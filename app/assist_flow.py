"""Recognized assist stages and process-owned controls."""
import asyncio
import hashlib
import logging
import re
import time
from urllib.parse import parse_qs, urlsplit, urlunsplit
from dataclasses import dataclass
from contextlib import suppress

from .assist import BOOKING_SELECTORS, CAPTCHA_SELECTORS, _document_key
from .classifier import NO_SLOT_MARKERS
from .logging_utils import log_event
from .config import REGRESO_PROCEDURE
from .page_inspection import dead_document, visible_documents
from .site import SiteCheckError, _challenge_code, _flow_endpoint, _is_icp_page, _support_id

log = logging.getLogger(__name__)
SERVER_ERROR_PAUSES = (30, 60, 120, 300)
LABELS = {
    "province": ("Выбор провинции", "Выбрать провинцию и перейти дальше"),
    "procedure": ("Выбор услуги", "Выбрать trámite и перейти дальше"),
    "auth": ("Вход con Cl@ve", "Войти с электронным сертификатом"),
    "auth_wait": ("Авторизация", "Дождаться возврата с сертификатом"),
    "applicant": ("Форма заявителя", "Заполнить данные и перейти дальше"),
    "applicant_no_slots": ("Сит нет на форме заявителя", "Подождать и обновить форму без ввода данных"),
    "request_menu": ("Меню запроса", "Нажать Solicitar cita"),
    "request_no_slots": ("Сит нет после запроса", "Подождать и обновить результат"),
    "request_no_slots_restart": ("Сит нет — новый проход", "Aceptar → начать заново"),
    "post_contact_no_slots": ("Сит нет после контактов", "Нажать Salir и начать заново"),
    "office": ("Выбор офиса", "Проверить доступные офисы"),
    "contact": ("Контактные данные", "Заполнить телефон и email — перейти дальше"),
    "booking": ("Выбор ситы", "Выбрать дату и время в окне сайта"),
    "captcha": ("Капча", "Пройти проверку в окне сайта"),
    "expired": ("Сессия истекла", "Следовать кнопке сообщения и начать заново"),
    "server_error": ("Ошибка сервера", "Подождать перед безопасным повтором"),
    "blocked": ("Доступ временно ограничен", "Подождать перед восстановлением"),
    "rate_limited": ("Слишком много запросов — HTTP 429", "Подождать 3 минуты и проверить доступ"),
    "loading": ("Загрузка страницы", "Дождаться видимого документа"),
    "unknown": ("Неизвестный экран", "Проверить страницу в окне сайта"),
}


@dataclass(frozen=True)
class Observed:
    kind: str
    document: str
    signature: str
    source: object
    evidence: str = ""
    recovery_action_first: bool = False
    certificate_validation_error: bool = False
    variant: str = ""
    operation_code: bool = False


def loading_placeholder(text):
    normalized=' '.join(text.casefold().split())
    return not normalized or normalized in {'proceso automático para la solicitud de cita previa','proceso automatico para la solicitud de cita previa'}


def no_slots_kind(*, applicant, after_contacts):
    if applicant:
        return "applicant_no_slots"
    return "post_contact_no_slots" if after_contacts else "request_no_slots"


def available_offices(options):
    return [o for o in options if not o["disabled"] and o["value"].strip() not in {"", "0", "-1"}
            and not re.search(r"selecci[oó]n|seleccione|seleccionar|elija|cualquier oficina", o["text"], re.I)]


class AssistRunner:
    def __init__(self, page, context, checker, task, run_id, control, alerter, policy):
        self.page, self.context, self.checker = page, context, checker
        self.task, self.run_id, self.control = task, run_id, control
        self.alerter, self.policy = alerter, policy
        self.last_action = None
        self.last_method = None
        self.pending_action = None
        self.pending_counted = False
        self.http_statuses = {}
        self.empty_streak = 0
        self.loading_started=None
        self.loading_document=None
        self.expired_attempts = 0
        self.expiry_immediate_attempted = set()
        self.server_attempts = 0
        self.server_button_attempts = 0
        self.manual_signature = None
        self._manual_auth_active = False
        self.action_phase = ""
        self.attempted = set()
        self.captures = {}
        self.attention = None
        self.clave_acknowledged = False
        self.last_observed = None
        self.timeline_state = None
        self.previous_stage = None
        self.pass_number = 1
        self.last_action_detail = ""
        self.current_offices = []
        self.office_observation_key = None
        self.office_selection_key = None
        self.selected_office = None
        self.booking_document = None
        self.handoff_observed = None
        self.booking_acknowledged = False
        self.guard_document = None
        self.guard_kind = None
        self.timeout_retries = {}
        self._listeners = []
        if hasattr(page, "on"):
            for event, handler in (("request", self._requested), ("response", self._responded)):
                page.on(event, handler)
                self._listeners.append((event, handler))
        checker._assist_guard = self.action_guard
        checker._assist_action = control.automation_action

    @property
    def timings(self):
        from .timings import AssistTimings
        return getattr(getattr(self.checker, 'settings', None), 'assist_timings', AssistTimings())

    def record(self, event, observed=None, detail="", *, page_url=None):
        if event in {"action", "recovery"}:
            self.last_action_detail = detail
        parts = urlsplit(page_url if page_url is not None else self.page.url)
        # Queries and matrix parameters may carry certificate/session tokens.
        path = "/".join(segment.split(";", 1)[0] for segment in parts.path.split("/"))
        url = urlunsplit((parts.scheme, parts.netloc, path, "", ""))
        return self.control.record(event, page=url, stage=observed.kind if observed else "", detail=detail,
                                   document=observed.document if observed else None)

    def log_availability(self, event, observed, *, page_url=None):
        if event == "offices_seen":
            province = getattr(self.task, 'province', '')
            self.control.observe_statistics('office', observed.document,
                offices=[f"{province}: {o['text']}" for o in self.current_offices])
        names = " · ".join(o["text"] for o in self.current_offices)
        if event == "offices_seen":
            detail = f"Доступные офисы ({len(self.current_offices)}): {names}"
        elif event == "booking_seen":
            detail = "Появился выбор даты/времени ситы"
        else:
            label = "Выбран офис" if event == "office_selected" else "Переход отправлен для офиса"
            detail = f"{label}: {self.selected_office['text']}"
        self.control.update_offices(self.current_offices, self.selected_office["text"] if self.selected_office else None)
        row = self.record(event, observed, detail, page_url=page_url)
        self.control.record_availability(event, page=row["page"], offices=self.current_offices,
                                         selected_office=self.selected_office, timestamp=row["time"])

    def office_selected(self, observed, value):
        office = next((o for o in self.current_offices if o["value"] == value), None)
        if office is None:
            return False
        self.selected_office = dict(value=office["value"], text=office["text"])
        key = (observed.document, value)
        if key != self.office_selection_key:
            self.office_selection_key = key
            self.log_availability("office_selected", observed)
        return True

    def save_error_details(self, exc):
        """Keep Playwright call logs private rather than copying them to the panel."""
        import json
        import os
        import traceback
        from datetime import datetime
        if self.control.diagnostics_enabled is False or not getattr(getattr(self.checker, "settings", None), "diagnostics_enabled", False):
            return
        try:
            path = self.checker._debug_directory(self.task.id, self.run_id) / "errors.jsonl"
            path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
            with os.fdopen(fd, "a", encoding="utf-8") as handle:
                if hasattr(os, "fchmod"):
                    os.fchmod(handle.fileno(), 0o600)
                handle.write(json.dumps({"time": datetime.now().astimezone().isoformat(),
                    "error_type": type(exc).__name__, "phase": self.action_phase,
                    "stage": self.last_observed.kind if self.last_observed else None,
                    "message": str(exc), "traceback": "".join(traceback.format_exception(exc))},
                    ensure_ascii=False) + "\n")
            self.record("capture", self.last_observed, "ERROR_DETAILS: errors.jsonl")
        except Exception:
            log_event(log, logging.WARNING, "assist_error_capture_failed", task_id=self.task.id)

    def _requested(self, request):
        if request.is_navigation_request() and request.frame == self.page.main_frame:
            if (request.method == "POST" and _flow_endpoint(request.url) == "acVerFormulario"
                    and self.last_observed is not None and self.last_observed.kind == "office"):
                values = parse_qs(getattr(request, "post_data", None) or "").get("idSede", [])
                if len(values) == 1 and self.office_selected(self.last_observed, values[0]):
                    self.log_availability("office_submitted", self.last_observed, page_url=request.url)
            self.last_method = request.method
            self.http_statuses.pop(self.page.main_frame, None)
            self.control.publish(screen="Загрузка страницы", evidence="Ожидаем результат перехода",
                                 next_action="Распознать новый документ", mode="working")
            if request.method == 'POST' and _flow_endpoint(request.url)=='acValidarEntrada':
                self.record('applicant_submitted',self.last_observed,'Отправлена форма заявителя — значения не сохраняются',page_url=request.url)
            if request.method == "POST" and _flow_endpoint(request.url) == "acCitar":
                self.control.availability_checked()
                self.record("check", self.last_observed, f"Проверка сит: отправлен Solicitar cita — в сессии {self.control.snapshot()['checks']}, для профиля {self.control.snapshot()['profile_checks']}", page_url=request.url)
            if (self.pending_action == "request" and not self.pending_counted
                    and request.method == "POST" and _flow_endpoint(request.url) == "acCitar"):
                self.control.request_sent()
                self.pending_counted = True

    def _responded(self, response):
        if response.request.is_navigation_request():
            self.http_statuses[response.request.frame] = (response.url, response.status)
            if response.request.frame == self.page.main_frame:
                self.control.response_observed(response.status)
            if response.request.frame == self.page.main_frame:
                self.record("response", self.last_observed,
                            f"{response.request.method} → HTTP {response.status} — документ получен, ожидаем распознавания",
                            page_url=response.url)

    async def wait_unpaused(self):
        while self.control.paused:
            await self.page.wait_for_timeout(250)

    async def visible(self, selector, source=None):
        locator = (source or self.page).locator(selector)
        return bool(await locator.count() and await locator.first.is_visible())

    async def observe(self):
        async def slow_read():
            await asyncio.sleep(0.5)
            self.control.publish(screen="Проверяем страницу", evidence="Чтение документа ещё не завершено",
                                 next_action="Дождаться распознавания нового состояния",
                                 mode="manual" if self.manual_signature is not None or self._manual_auth_active else "waiting")
        notice = asyncio.create_task(slow_read())
        try:
            return await self._observe()
        finally:
            notice.cancel()
            with suppress(asyncio.CancelledError):
                await notice

    async def _observe(self):
        document = await _document_key(self.page)
        docs = await visible_documents(self.page)
        signature = document + ":" + hashlib.sha256("\n".join(text for _, text in docs).encode()).hexdigest()
        kind, source, evidence = "unknown", self.page.main_frame, "Ожидаемые элементы не найдены"
        recovery_action_first = False
        certificate_validation_error = False
        rate_frame = next((frame for frame, _ in docs
            if self.http_statuses.get(frame) == (frame.url, 429)), None)
        if rate_frame is not None:
            kind, source, evidence = 'rate_limited', rate_frame, 'HTTP 429: сервер просит снизить частоту запросов'
        for frame, text in ([] if rate_frame is not None else docs):
            from .site import _dead_page_kind
            dead = _dead_page_kind(text)
            response_url, status = self.http_statuses.get(frame, (None, 0))
            if response_url == frame.url and status == 429:
                kind, source, evidence = 'rate_limited', frame, 'HTTP 429: сервер просит снизить частоту запросов'
                break
            if not dead and response_url == frame.url and status >= 500:
                dead = "server_500"
            if dead:
                kind = "expired" if dead == "session_expired" else "server_error"
                source = frame
                evidence = "Сообщение в основном документе" if frame == self.page.main_frame else "Сообщение во вложенном документе"
                from .site import _certificate_service_error
                certificate_validation_error = kind == "server_error" and _certificate_service_error(text)
                if certificate_validation_error:evidence="Сбой сервиса проверки сертификата @Firma"
                if kind == "expired":
                    normalized = " ".join(text.lower().split())
                    recovery_action_first = "la provincia seleccionada no ofrece el servicio de cita previa internet" in normalized
                    if recovery_action_first:
                        evidence = "Провинция не предоставляет услугу: на этом же экране сессия истекла — следовать Volver"
                    break
        if kind == "unknown":
            text = " ".join(docs[0][1].lower().split()) if docs else ""
            if not await self.visible("body"):
                result = Observed("loading", document, signature, source, "Документ ещё скрыт при загрузке")
                self.last_observed = result
                return result
            challenge = _challenge_code(text)
            if challenge == "HTTP_BLOCKED":
                kind, evidence = "blocked", "Сайт ограничил доступ"
                if 'request rejected' in text or 'requested url was rejected' in text:
                    evidence = 'Request Rejected: защита сайта отклонила запрос'
                support = _support_id(text)
                if support:
                    evidence += f'; Support ID: {support}'
            elif not any(marker in text for marker in NO_SLOT_MARKERS) and any([await self.visible(s) for s in BOOKING_SELECTORS]):
                # A booking form may contain its own CAPTCHA. Its date/time
                # controls still require permanent ownership by the user.
                kind, evidence = "booking", "Видимые поля даты или времени"
            elif challenge == "CAPTCHA_DETECTED" or any([await self.visible(s) for s in CAPTCHA_SELECTORS]):
                kind, evidence = "captcha", "Видимая проверка капчи"
            else:
                applicant = await self.visible("#txtIdCitado")
                if applicant:
                    applicant = (await self.page.locator("#txtIdCitado").first.is_editable()
                                 or (getattr(self.task,'applicant_fill_mode','manual')=='copy'
                                     and await self.visible('#btnCopiar') and await self.visible('#btnEnviar')))
                no_slots = any(m in text for m in NO_SLOT_MARKERS)
                if await self.visible("#form"):
                    kind, evidence = "province", "Виден список провинций"
                elif await self.visible('select[id^="tramiteGrupo["]:visible'):
                    kind, evidence = "procedure", "Виден список услуг — предупреждение не является результатом запроса"
                elif await self.visible("#btnAccesoClave"):
                    kind, evidence = "auth", "Доступна кнопка con Cl@ve"
                elif applicant:
                    kind = "applicant_no_slots" if no_slots else "applicant"
                    evidence = ("На форме заявителя нет сит — ожидать без ввода данных" if no_slots
                                else "Видимая форма заявителя")
                elif no_slots:
                    kind = no_slots_kind(applicant=False, after_contacts=(self.pending_action or self.last_action) == "contact")
                    evidence = "Видимое сообщение «нет сит»"
                    if kind == "request_no_slots" and await self.visible('form[action="salirInicio"] #btnSubmit'):
                        button = self.page.locator('form[action="salirInicio"] #btnSubmit').first
                        label = (await button.get_attribute("value") or await button.inner_text()).strip()
                        if re.fullmatch(r"aceptar", label, re.I):
                            kind = "request_no_slots_restart"
                            evidence = "Сит нет. Aceptar завершает текущий проход; быстрое обновление не требуется"
                elif _flow_endpoint(self.page.url) in {"acValidarEntrada","acOpcDirect"} and await self.visible("#btnEnviar"):
                    button = self.page.locator("#btnEnviar").first
                    label = (await button.get_attribute("value") or await button.inner_text()).lower()
                    if "solicitar" in label:
                        kind, evidence = "request_menu", "Доступна Solicitar cita"
                elif await self.visible("#idSede") or await self.visible("select[name='idSede']"):
                    kind, evidence = "office", "Виден селектор доступных офисов"
                elif all([await self.visible(s) for s in ("#txtTelefonoCitado", "#emailUNO", "#emailDOS")]):
                    kind, evidence = "contact", "Видны телефон и два поля email"
                elif not _is_icp_page(self.page.url):
                    kind, evidence = "auth_wait", "Открыт портал авторизации"
        if kind == 'unknown' and loading_placeholder(text):
            kind, evidence = 'loading', 'Пустой документ: ожидается ответ сайта'
        if document != await _document_key(self.page):
            raise SiteCheckError("SCREEN_CHANGED", True)
        variant = ''
        if kind == 'unknown' and not self.control.booking_handoff:
            # Only element counts/types, never values, labels, IDs or page text.
            shape = await source.evaluate("""() => ({forms:document.forms.length,
                selects:document.querySelectorAll('select').length,
                buttons:document.querySelectorAll('button,input[type=submit]').length,
                inputs:['text','password','checkbox','radio','email','tel','date'].map(
                    type=>document.querySelectorAll('input[type="'+type+'"]').length)})""")
            import json
            variant = hashlib.sha256(json.dumps(shape,sort_keys=True).encode()).hexdigest()
        operation_code = kind in {'applicant_no_slots','request_no_slots','request_no_slots_restart','post_contact_no_slots'} and bool(re.search(r'cod\.?\s*oper',text,re.I))
        result = Observed(kind, document, signature, source, evidence, recovery_action_first, certificate_validation_error, variant, operation_code)
        self.last_observed = result
        if kind in {'booking', 'expired', 'server_error', 'rate_limited', 'blocked'}:
            self.control.observe_statistics(kind, document)
        return result

    def publish(self, observed, *, next_action=None, mode="working", remaining=None):
        if observed.kind=='auth_wait' and mode=='manual':
            self._manual_auth_active=True
        elif observed.kind not in {'auth_wait','loading','unknown'}:
            self._manual_auth_active=False
        title, action = LABELS[observed.kind]
        if observed.certificate_validation_error:title="Ошибка проверки сертификата"
        if observed.kind == 'auth_wait':
            title = ('Cl@ve Permanente' if getattr(self.task,'clave_method','movil') == 'permanente' else 'Cl@ve Móvil') if getattr(self.task,'access_mode','certificate') == 'clave' else 'Авторизация сертификатом'
        transition = (observed.document, observed.kind)
        if observed.kind == "province" and self.previous_stage != "province":
            self.control.observe_statistics("pass", observed.document)
            if self.previous_stage is None:
                self.record("flow_start", observed, "Проход №1: начальная форма выбора провинции")
            else:
                self.pass_number += 1
                previous = LABELS[self.previous_stage][0]
                reasons = {"expired": "Восстановление после сообщения об истёкшей сессии",
                           "server_error": "Возврат после серверной ошибки",
                           "blocked": "Возврат после блокировки доступа",
                           "request_no_slots_restart": "Нет сит: сайт предлагает завершить проход через Aceptar",
                           "post_contact_no_slots": "Перезапуск после отсутствия сит на этапе контактов"}
                reason = reasons.get(self.previous_stage,
                    "Сайт вернул начальную форму — отдельное сообщение о причине не распознано")
                detail = f"Проход №{self.pass_number}: {previous} → Выбор провинции. {reason}"
                if self.last_action_detail:
                    detail += f". Последнее действие: {self.last_action_detail}"
                response_url, status = self.http_statuses.get(getattr(self.page, "main_frame", None), (None, None))
                if response_url == self.page.url and status is not None:
                    detail += f". Ответ: HTTP {status}"
                self.record("flow_restart", observed, detail)
        parts = urlsplit(self.page.url)
        path = "/".join(segment.split(";", 1)[0] for segment in parts.path.split("/"))
        self.control.observe_visit(document=observed.document, stage=observed.kind,
            page=urlunsplit((parts.scheme, parts.netloc, path, "", "")), title=title,
            recognition=observed.kind if observed.kind in {"unknown", "loading"} else "known", variant=observed.variant,
            automated=mode!='manual' and not self._manual_auth_active,
            evidence=('no_appointments','operation_code') if observed.kind in {'applicant_no_slots','request_no_slots','request_no_slots_restart','post_contact_no_slots'} and observed.operation_code else ())
        if observed.kind not in {"loading", "auth_wait"}:
            self.previous_stage = observed.kind
        if observed.kind == "province":
            self.current_offices = []
            self.selected_office = None
        if observed.kind == "booking" and self.booking_document != observed.document:
            self.booking_document = observed.document
            self.log_availability("booking_seen", observed)
        if transition != self.timeline_state:
            self.record("screen", observed, observed.evidence)
            self.timeline_state = transition
        self.control.publish(screen=title, evidence=observed.evidence, next_action=next_action or action,
                             mode=mode, remaining_seconds=remaining)

    async def save(self, observed, code):
        if self.control.diagnostics_enabled is False:
            return
        key = (observed.signature, code)
        if self.captures.get(key, 0) >= 2:
            return
        self.captures[key] = self.captures.get(key, 0) + 1
        if await self.checker._save_debug(self.page, self.task.id, self.run_id,
                                        error_code=code, error_field=observed.kind, include_controls=True):
            self.captures[key] = 2
            self.record("capture", observed, code)

    async def signal(self, observed, reason, *, repeat):
        if not self.control.sound_enabled:
            await self.silence()
            return
        if reason == "clave_authentication" and self.clave_acknowledged:
            return
        key = ("clave_interactive" if reason == "clave_authentication" else observed.signature, reason)
        if self.attention != key:
            await self.alerter.start(reason, repeat=repeat)
            self.attention = key

    async def clave_human_attention(self):
        # Acknowledgement survives QR refreshes and redirects within this login.
        return await self.page.evaluate("""() => {
            const root = document.documentElement;
            if (!root.hasAttribute('data-cita-clave-attention-installed')) {
                root.setAttribute('data-cita-clave-attention-installed','1');
                const mark = e => { if (e.isTrusted) root.setAttribute('data-cita-clave-human-attention','1'); };
                document.addEventListener('pointerdown',mark,true);
                document.addEventListener('keydown',mark,true);
                window.addEventListener('focus',mark,true);
                document.addEventListener('visibilitychange',e => {
                    if (document.visibilityState === 'visible') mark(e);
                },true);
            }
            return root.hasAttribute('data-cita-clave-human-attention');
        }""")

    async def silence(self):
        if self.attention is not None:
            await self.alerter.stop()
            self.attention = None

    async def action_guard(self, _page=None):
        await self.wait_unpaused()
        observed = await self.observe()
        self.publish(observed)
        if observed.kind in {"expired", "server_error", "blocked", "rate_limited"}:
            raise SiteCheckError({"expired": "SESSION_EXPIRED", "server_error": "SITE_TEMPORARY_ERROR",
                                 "blocked": "HTTP_BLOCKED", "rate_limited": "HTTP_RATE_LIMITED"}[observed.kind], True)
        if self.guard_document and (observed.document != self.guard_document or observed.kind != self.guard_kind):
            raise SiteCheckError("SCREEN_CHANGED", True)

    async def guard(self, observed):
        await self.wait_unpaused()
        current = await self.observe()
        return current.signature == observed.signature and current.kind == observed.kind

    async def countdown(self, observed, delay, next_action):
        self.record("wait", observed, f"{next_action}; {delay:.1f} секунд")
        remaining = float(delay)
        previous = time.monotonic()
        while True:
            current = await self.observe()
            if current.signature != observed.signature or current.kind != observed.kind:
                return False
            now = time.monotonic()
            if not self.control.paused:
                remaining -= now - previous
            previous = now
            self.publish(current, next_action=next_action, mode="waiting", remaining=max(0, remaining))
            if remaining <= 0 and not self.control.paused:
                return await self.guard(current)
            await self.page.wait_for_timeout(250)

    async def click(self, observed, selector, *, source=None, action=None):
        if not await self.guard(observed):
            return False
        await self.save(observed, "STEP_BEFORE_ACTION")
        if not await self.guard(observed):
            return False
        self.record("action", observed, action or selector)
        self.pending_action = action
        self.pending_counted = False
        from playwright.async_api import TimeoutError as PlaywrightTimeoutError
        locator = (source or observed.source).locator(selector).first
        self.action_phase = f"{action or observed.kind}.click"
        try:
            await self.checker._activate_control(locator, timeout=5_000)
        except PlaywrightTimeoutError as exc:
            message = str(exc)
            pre_dispatch = ("waiting for element to be visible, enabled and stable" in message
                            and "performing click action" not in message
                            and "click action done" not in message)
            if action not in {"province", "procedure"} or not pre_dispatch:
                raise
            if not await self.guard(observed):
                return False
            # Read-only hit test: use trusted keyboard activation only for an
            # unobstructed control in the same recognized document.
            unobstructed = await locator.evaluate("""el => {
                const r=el.getBoundingClientRect(), x=r.x+r.width/2, y=r.y+r.height/2;
                const hit=document.elementFromPoint(x,y);
                return r.width>0 && r.height>0 && !!hit && (hit===el || el.contains(hit));
            }""")
            if not unobstructed:
                raise
            self.save_error_details(exc)
            self.record("interaction_retry", observed, "Клик не был отправлен: активация кнопки клавишей Enter")
            await self.checker._activate_control(locator, timeout=5_000, keyboard=True)
        if action:
            self.last_action = action
        self.pending_action = None
        self.record("action_completed", observed, action or selector)
        # Same-URL POSTs also replace the document. Do not wait for a new URL,
        # nor resend just because a navigation/response was slow.
        try:
            await self.page.wait_for_load_state("domcontentloaded", timeout=5_000)
        except Exception:
            pass
        await self.page.wait_for_timeout(250)
        return True

    async def continuation(self, source, pattern):
        candidates = []
        buttons = source.locator("button, input[type='submit'], input[type='button'], a[href]")
        for index in range(await buttons.count()):
            button = buttons.nth(index)
            if not await button.is_visible() or not await button.is_enabled():
                continue
            label = (await button.get_attribute("value") or await button.inner_text()).strip()
            if re.fullmatch(pattern, label, re.I):
                candidates.append(button)
        return candidates[0] if len(candidates) == 1 else None

    async def expiry_scope(self, source, dead_kind="session_expired"):
        # A nested expiry message must own its recovery button, even when the
        # surrounding expected form still has another Aceptar button.
        from .site import _dead_page_kind
        containers = source.locator("div, section, article, dialog, form, body")
        matches = []
        for index in range(await containers.count()):
            region = containers.nth(index)
            if not await region.is_visible():
                continue
            if _dead_page_kind(await region.inner_text()) != dead_kind:
                continue
            if not await region.locator("button, input[type='submit'], input[type='button'], a[href]").count():
                continue
            matches.append((await region.locator("*").count(), index))
        return containers.nth(min(matches)[1]) if matches else source

    async def record_recovery_button(self, button, observed):
        # Reading a caption is telemetry: it must never prevent recovery.
        with suppress(Exception):
            label = (await button.get_attribute('value') or await button.inner_text()).strip()
            if label:
                self.record('recovery_button', observed, label)
            return label
        return ''

    async def hold(self, observed, reason):
        self.record("manual_required", observed, reason)
        self.manual_signature = observed.signature
        self.publish(observed, next_action=reason, mode="manual")
        await self.save(observed, "MANUAL_REQUIRED")
        await self.signal(observed, reason, repeat=True)
        await self.page.wait_for_timeout(500)

    async def recover(self, observed):
        if observed.kind == 'rate_limited':
            from .config import DEFAULT_START_URL
            await self.silence()
            await self.save(observed, 'HTTP_RATE_LIMITED')
            if not await self.countdown(observed, self.timings.recovery_delay(self.timings.rate_limit_seconds), 'Проверить доступ после HTTP 429'):
                return
            # Never replay a contact/booking POST. A single GET probes the site
            # using the existing context and cookies; repeated 429 waits again.
            target = self.page.url if self.last_method == 'GET' else DEFAULT_START_URL
            self.record('recovery', observed, 'Проба после HTTP 429: один GET — профиль и cookies сохранены')
            self.last_action = self.pending_action = None
            await self.page.goto(target, wait_until='domcontentloaded')
            self.control.automation_action()
            return
        if observed.kind == 'blocked':
            from .assist_runtime import AssistRestartRequested
            await self.silence()
            await self.save(observed, 'HTTP_BLOCKED')
            delay = self.timings.recovery_delay(self.timings.blocked_seconds, self.control.blocked_streak)
            self.record('recovery', observed, observed.evidence or 'Сайт ограничил доступ')
            if not await self.countdown(observed, delay, 'После кулдауна очистить сессию и начать заново'):
                return
            self.control.blocked_streak += 1
            self.record('recovery', observed, 'Блокировка: закрыть браузерную сессию и очистить её хранилище')
            raise AssistRestartRequested('блокировка сайта', clear_profile=True, delay_seconds=0,cooldown_completed=True)
        await self.silence()
        await self.save(observed, "SESSION_EXPIRED_SCREEN" if observed.kind == "expired" else "SERVER_ERROR_SCREEN")
        if observed.kind == 'server_error' and urlsplit(self.page.url).hostname in {'pasarela.clave.gob.es','pasarela-ident.clave.gob.es'}:
            from .assist_runtime import AssistRestartRequested
            self.record('recovery',observed,'Ошибка Cl@ve: ждём перед закрытием окна — профиль и cookies сохраняются')
            if not await self.countdown(observed,self.timings.clave_error_seconds,'Закрыть Camoufox и начать заново без очистки профиля'):
                return
            raise AssistRestartRequested('ошибка портала Cl@ve — профиль и cookies сохранены',reopen_browser=True,delay_seconds=0,cooldown_completed=True)
        if observed.kind == "expired":
            self.expired_attempts += 1
            delay = self.timings.recovery_delay(self.timings.expired_seconds, self.expired_attempts - 1)
            action_first = observed.recovery_action_first and observed.document not in self.expiry_immediate_attempted
            if not action_first and not await self.countdown(observed, delay, "Повторить штатное восстановление сессии"):
                return
            self.publish(observed, mode="recovery")
            scope = await self.expiry_scope(observed.source)
            button = await self.continuation(scope, r"(?:aceptar|volver|salir|reiniciar|reiniciar sesión|iniciar|restart|return)")
            if button is None:
                await self.hold(observed, "Не распознана кнопка восстановления. Следуй сообщению на сайте")
                return
            if not await self.guard(observed):
                return
            self.action_phase = "expired.continue"
            label = await self.record_recovery_button(button, observed)
            self.record("recovery", observed, f"Кнопка сообщения об истёкшей сессии{': ' + label if label else ''}")
            if action_first:
                self.expiry_immediate_attempted.add(observed.document)
            await self.checker._activate_control(button, timeout=5_000)
            await self.page.wait_for_timeout(500)
            self.last_action = None
            self.pending_action = None
            # The embedded message's own procedure has priority. Reinspect
            # its result before considering an explicit return to entry.
            after = await self.observe()
            if after.signature == observed.signature:
                self.record("recovery", after, "Кнопка не изменила экран — следующая попытка после задержки")
            elif observed.source != self.page.main_frame and after.document == observed.document:
                await self.hold(after, "Восстановление продолжилось во вложенном окне. Проверить его перед продолжением")
            if action_first and after.kind == 'province':
                await self.countdown(after, delay, 'Пауза перед новым проходом после истёкшей сессии')
            return
        if observed.kind == "server_error" and observed.source is not None and _is_icp_page(self.page.url):
            scope = await self.expiry_scope(observed.source, dead_kind="server_500")
            button = await self.continuation(scope, r"(?:aceptar|volver|salir|reiniciar|iniciar|restart|return)")
            if button is not None:
                delay = self.timings.recovery_delay(self.timings.server_button_seconds, self.server_button_attempts)
                if observed.certificate_validation_error:
                    delay=max(delay,self.timings.recovery_delay(self.timings.clave_error_seconds))
                    self.record("recovery",observed,"Сервис @Firma не проверил сертификат — пауза перед повторной авторизацией")
                if not await self.countdown(observed, delay, "Выполнить кнопку восстановления на странице ошибки"):
                    return
                if not await self.guard(observed):
                    return
                self.server_button_attempts += 1
                self.action_phase = "server_error.continue"
                self.record("recovery", observed, "Нажать кнопку сообщения об ошибке и перезапустить")
                await self.record_recovery_button(button, observed)
                await self.checker._activate_control(button, timeout=5_000)
                self.last_action = self.pending_action = None
                await self.page.wait_for_timeout(500)
                after = await self.observe()
                if after.signature == observed.signature:
                    self.record("recovery", after, "Кнопка не изменила экран — следующая попытка после задержки")
                elif observed.source != self.page.main_frame and after.document == observed.document:
                    await self.hold(after, "Восстановление продолжилось во вложенном окне. Проверить его перед продолжением")
                return
        self.server_attempts += 1
        delay = self.timings.recovery_delay(self.timings.server_retry_seconds, self.server_attempts - 1)
        if observed.certificate_validation_error:
            delay=max(delay,self.timings.recovery_delay(self.timings.clave_error_seconds))
        if observed.kind == "blocked":
            # WAF recovery uses the established 5/6/7/9/10 minute rhythm.
            delay = (300, 360, 420, 540, 600)[min(self.server_attempts - 1, 4)]
        if not await self.countdown(observed, delay, "Проверить ответ и безопасно повторить загрузку"):
            return
        if self.last_method == "GET":
            await self.page.reload(wait_until="domcontentloaded")
            self.control.automation_action()
        elif observed.kind != "blocked" and _flow_endpoint(self.page.url) == "acCitar" and (self.pending_action or self.last_action) == "request":
            self.pending_action = "request"
            self.pending_counted = False
            self.record("recovery", observed, "Повторить поиск сит после серверной ошибки")
            await self.page.reload(wait_until="domcontentloaded")
            self.control.automation_action()
            self.pending_action = None
        elif observed.kind != "blocked" and (self.pending_action or self.last_action) != "contact" and (not _is_icp_page(self.page.url) or _flow_endpoint(self.page.url) in {"acInfo", "acEntrada", "procesarRespuestaClave"}):
            from .config import DEFAULT_START_URL
            self.record("recovery", observed, "Безопасный GET начальной страницы после сбоя авторизации")
            await self.page.goto(DEFAULT_START_URL, wait_until="domcontentloaded")
            self.control.automation_action()
            self.pending_action = None
            self.last_action = None
        elif (_is_icp_page(self.page.url)
              and _flow_endpoint(self.page.url) in {"index", "index.html"}
              and (self.pending_action or self.last_action) == "no_slots_restart"):
            # Aceptar only ends the no-slots pass. Do not replay that POST;
            # load the entry document with GET and reuse this browser profile.
            from .config import DEFAULT_START_URL
            self.record("recovery", observed, "Ошибка при завершении прохода — открыть начальную страницу через GET")
            await self.page.goto(DEFAULT_START_URL, wait_until="domcontentloaded")
            self.control.automation_action()
            self.last_action = self.pending_action = None
        else:
            await self.hold(observed, "Неопределённый результат отправки. Проверить ответ вручную — повторная отправка отключена")

    async def wait_loading(self,observed):
        from .assist_runtime import AssistRestartRequested
        now=time.monotonic()
        if self.loading_document!=observed.document or self.loading_started is None:
            self.loading_document=observed.document;self.loading_started=now
        if now-self.loading_started<self.timings.navigation_seconds:
            self.publish(observed,mode='waiting',next_action='Дождаться ответа сайта');await self.page.wait_for_timeout(500);return
        await self.silence();await self.save(observed,'SITE_LOADING_TIMEOUT')
        self.record('recovery',observed,'Сайт не ответил в пределах таймаута: пауза и новый проход без повторной отправки формы')
        if await self.countdown(observed,self.timings.recovery_delay(self.timings.server_retry_seconds),'Сайт не отвечает — после паузы начать новый проход'):
            raise AssistRestartRequested('таймаут ответа сайта — новый проход',clear_profile=False,delay_seconds=0,cooldown_completed=True)

    async def step(self):
        self.control.touch()
        if self.control.booking_handoff and self.handoff_observed is not None:
            # Ownership stays with the user even after navigation or an error.
            # Only explicit session restart can start another automatic pass.
            await self.booking_attention(self.handoff_observed)
            return
        observed = await self.observe()
        self.action_phase = ""
        if observed.kind!="loading":self.loading_started=None;self.loading_document=None
        if observed.signature == self.manual_signature:
            self.publish(observed, next_action="Ожидаем ручного перехода на сайте", mode="manual")
            await self.page.wait_for_timeout(500)
            return
        self.publish(observed, mode="manual" if observed.kind in {"booking", "captcha"} else "working")
        if observed.kind in {"booking", "captcha"}:
            # Notify before diagnostics, which may be slow or fail.
            await self.booking_attention(observed)
            return
        log_event(log, logging.INFO, "assist_screen_recognized", task_id=self.task.id,
                  run_id=self.run_id, stage=observed.kind)
        await self.save(observed, "TRACE_SCREEN")
        self.manual_signature = None
        if observed.kind in {"applicant", "applicant_no_slots", "office", "contact", "booking", "request_no_slots", "request_no_slots_restart", "post_contact_no_slots"}:
            self.control.blocked_streak = 0
            self.expired_attempts = self.server_attempts = self.server_button_attempts = 0
        if observed.kind in {"expired", "server_error", "blocked", "rate_limited"}:
            await self.recover(observed)
            return
        if observed.kind == "auth_wait" and getattr(self.task,"access_mode",None)=="clave":
            key=(observed.document,"clave_method")
            if key not in self.attempted:
                if await self.checker._select_clave_method(self.page,self.task):
                    self.attempted.add(key)
                    await self.silence()
                    self.record("action",observed,"Выбрать "+("Cl@ve Permanente" if getattr(self.task,"clave_method","movil")=="permanente" else "Cl@ve Móvil"))
                    return
            if not (self.attention and self.attention[1]=="clave_authentication") and not await self.checker._clave_requires_attention(self.page):
                self.publish(observed,mode="waiting")
                await self.page.wait_for_timeout(500)
                return
            self.publish(observed,next_action="Выполнить вход через Cl@ve в Camoufox",mode="manual")
            if await self.clave_human_attention():
                self.clave_acknowledged = True
                await self.silence()
            if not self.clave_acknowledged:
                await self.signal(observed,"clave_authentication",repeat=True)
            await self.page.wait_for_timeout(500)
            return
        if observed.kind not in {"booking", "captcha", "office"}:
            await self.silence()
        if observed.kind == "unknown":
            # One bounded reread lets a committing document finish loading.
            await self.page.wait_for_timeout(500)
            after = await self.observe()
            if after.signature == observed.signature and after.kind == "unknown":
                await self.hold(after, "Неизвестная страница. Проверить её и продолжить вручную")
            return
        if observed.kind=='loading':
            await self.wait_loading(observed);return
        if observed.kind == 'auth_wait':
            self.publish(observed, mode="waiting")
            await self.page.wait_for_timeout(500)
            return
        if not await self.guard(observed):
            return
        self.guard_document, self.guard_kind = observed.document, observed.kind
        try:
            if await self.checker._accept_cookies(self.page):
                # Dismissing the banner changes the document signature. Read
                # it again on the next tick before choosing any form action.
                return
            await self.act(observed)
        finally:
            self.guard_document = self.guard_kind = None

    async def act(self, observed):
        kind = observed.kind
        if kind == "province":
            self.last_action = None
            self.pending_action = None
            self.record("action", observed, "Выбрать провинцию")
            await self.checker._select_label(self.page, "#form", self.task.province, "request.province")
            await self.click(await self.observe(), "#btnAceptar", action="province")
        elif kind == "procedure":
            self.record("action", observed, "Выбрать услугу")
            await self.checker._select_procedure(self.page, self.task.procedure)
            await self.click(await self.observe(), "#btnAceptar", action="procedure")
        elif kind == "auth":
            self.guard_document = self.guard_kind = None
            async def attention(reason, repeat=False):
                waiting = reason == 'certificate_wait'
                self.control.publish(screen="Авторизация сертификатом",
                                     evidence="Авторизация длится больше 5 секунд" if waiting else "Портал запросил подтверждение",
                                     next_action="Проверить браузер — возможно, требуется выбор сертификата" if waiting else "Подтвердите сертификат, если браузер показывает запрос", mode="manual")
                if self.control.sound_enabled:
                    await self.alerter.start(reason, repeat=False)
                self.attention = (observed.signature, reason)
            if getattr(self.task, 'access_mode', 'certificate') == 'clave':
                self.clave_acknowledged = False
                self.record('action', observed, 'Войти через Cl@ve')
                try:
                    await self.checker._enter_with_clave(self.page, self.task, self.run_id)
                except SiteCheckError as exc:
                    if exc.code != 'AUTH_REQUIRED':
                        raise
                    self.publish(observed, next_action='Выполнить вход через Cl@ve в Camoufox', mode='manual')
                    await self.signal(observed,"clave_authentication",repeat=True)
                    return
            else:
                self.record("action", observed, "Войти с электронным сертификатом")
                await self.checker._enter_with_certificate(self.page, self.task, self.run_id, on_attention=attention)
            await self.silence()
        elif kind == "applicant":
            if (observed.document, kind) in self.attempted:
                await self.hold(observed, "Форма уже отправлялась. Проверить ответ и данные")
                return
            if getattr(self.task, 'applicant_fill_mode', 'manual') == 'copy':
                self.record("action", observed, "Скопировать документ и имя из авторизации" if getattr(self.task,'procedure',None)==REGRESO_PROCEDURE else "Скопировать NIE и имя из авторизации")
                await self.checker._fill_applicant_with_clave(self.page, self.task, self.run_id)
            else:
                self.record("action", observed, "Ввести документ и имя из профиля" if getattr(self.task,'procedure',None)==REGRESO_PROCEDURE else "Ввести NIE и имя из профиля")
                await self.checker._fill_applicant_manual(self.page, self.task, self.run_id)
            current = await self.observe()
            if current.kind != "applicant" or current.document != observed.document:
                return
            self.attempted.add((observed.document, kind))
            await self.click(current, "#btnEnviar", action="applicant")
        elif kind == "request_menu":
            if (observed.document, kind) in self.attempted:
                await self.hold(observed, "Запрос уже отправлен. Проверить ответ сайта")
                return
            if await self.click(observed, "#btnEnviar", action="request"):
                self.attempted.add((observed.document, kind))
        elif kind == "request_no_slots_restart":
            attempt = (observed.document, "no_slots_restart")
            if attempt in self.attempted:
                await self.hold(observed, "Перезапуск уже отправлялся. Проверить ответ сайта")
                return
            if not await self.countdown(observed, self.timings.no_slots_restart_seconds, "Aceptar → начать заново"):
                return
            if await self.click(observed, 'form[action="salirInicio"] #btnSubmit', action="no_slots_restart"):
                self.attempted.add(attempt)
        elif kind in {"applicant_no_slots", "request_no_slots"}:
            applicant_wait = kind == "applicant_no_slots"
            attempt = (observed.document, "applicant_reload" if applicant_wait else "request_reload")
            if attempt in self.attempted:
                await self.hold(observed, "Повтор уже отправлялся. Ожидаем новый ответ — повторная отправка остановлена")
                return
            self.empty_streak += 1
            delay = self.policy.next_delay(recent_activity=False, empty_streak=self.empty_streak)
            via_salir=not applicant_wait and getattr(self.task,'no_slots_mode','salir')=='salir'
            next_action = ("Обновить форму без ввода данных" if applicant_wait else
                           "Нажать Salir и продолжить по сайту" if via_salir else
                           "Обновить результат запроса в текущей сессии")
            if not await self.countdown(observed, delay, next_action):
                return
            await self.save(observed, "STEP_BEFORE_ACTION")
            if not await self.guard(observed):
                return
            if via_salir:
                button=await self.continuation(observed.source,r"salir")
                if button is None:
                    await self.hold(observed,"Не найдена единственная кнопка Salir. Проверить страницу")
                    return
                if not await self.guard(observed):return
                self.attempted.add(attempt)
                self.pending_action=self.last_action='no_slots_salir'
                self.pending_counted=False
                self.action_phase='no_slots.salir'
                self.record('action',observed,'Нажать Salir: продолжить по предложенному сайтом пути')
                await self.checker._activate_control(button,timeout=5_000)
                self.pending_action=None
                self.record('action_completed',observed,'Salir отправлен — распознать ответ сайта')
                return
            self.attempted.add(attempt)
            self.pending_action = "applicant_reload" if applicant_wait else "request"
            self.pending_counted = False
            self.last_action = self.pending_action
            self.record("action", observed, "Обновить форму без ввода данных" if applicant_wait else "Обновить результат: повтор POST acCitar")
            await self.page.reload(wait_until="domcontentloaded", timeout=self.checker.settings.navigation_timeout_seconds * 1000)
            self.control.automation_action()
            self.pending_action = None
            self.record("action_completed", observed, "Обновление завершено — распознать ответ")
        elif kind == "post_contact_no_slots":
            if (observed.document, kind) in self.attempted:
                await self.hold(observed, "Перезапуск уже отправлялся. Проверить ответ сайта")
                return
            button = await self.continuation(observed.source, r"salir")
            if button is None and _flow_endpoint(self.page.url) == "acOfertarCita":
                candidate = observed.source.locator("#btnSubmit")
                if await candidate.count() and await candidate.first.is_visible() and await candidate.first.is_enabled():
                    label = (await candidate.first.get_attribute("value") or await candidate.first.inner_text()).strip()
                    if re.fullmatch(r"aceptar", label, re.I):
                        button = candidate.first
            if button is None:
                await self.hold(observed, "Сит нет после контактов — не найдена кнопка перезапуска Salir/Aceptar")
                return
            if await self.guard(observed):
                await self.save(observed, "STEP_BEFORE_ACTION")
                if not await self.guard(observed):
                    return
                self.attempted.add((observed.document, kind))
                self.action_phase = "post_contact.restart"
                self.record("action", observed, "Перезапуск после отсутствия сит: Salir/Aceptar")
                await self.checker._activate_control(button, timeout=5_000)
                self.pending_action = None
                await self.page.wait_for_timeout(500)
                current = await self.observe()
                if current.document != observed.document and current.kind == "province":
                    self.last_action = None
                    self.record("action_completed", current, "Перезапуск подтверждён: выбор провинции")
        elif kind == "office":
            await self.office(observed)
        elif kind == "contact":
            if (observed.document, kind) in self.attempted:
                await self.hold(observed, "Контакты уже отправлялись. Проверить ответ сайта")
                return
            self.record("action", observed, "Заполнить контактные поля")
            if not await self.checker._fill_contact(self.page, self.task, self.run_id):
                current = await self.observe()
                if current.kind == "contact":
                    await self.hold(current, "Контакты не заполнены. Проверить поля вручную")
                return
            self.attempted.add((observed.document, kind))
            await self.click(await self.observe(), "#btnSiguiente", action="contact")

    async def office(self, observed):
        await self.save(observed, "OFFICE_SELECTION_SCREEN")
        # The user may submit the office while diagnostics are being saved.
        # Read value/options atomically without waiting for a vanished selector.
        snapshot = await self.page.evaluate("""() => {
            const select = document.querySelector("#idSede, select[name='idSede']");
            return select ? {value: select.value, options: Array.from(select.options, o =>
                ({value:o.value,text:o.text,disabled:!!(o.disabled || o.parentElement.disabled)}))} : null;
        }""")
        if snapshot is None or not await self.guard(observed):
            await self.silence()
            return
        select = self.page.locator("#idSede, select[name='idSede']").first
        offices = available_offices(snapshot['options'])
        key = (observed.document, tuple((o["value"], o["text"]) for o in offices))
        if key != self.office_observation_key:
            self.office_observation_key = key
            self.current_offices = [dict(value=o["value"], text=o["text"]) for o in offices]
            self.selected_office = None
            self.office_selection_key = None
            if offices:
                self.log_availability("offices_seen", observed)
        selected_value = snapshot['value']
        self.office_selected(observed, selected_value)
        if len(offices) != 1:
            if not offices:
                await self.hold(observed, "Нет достоверного доступного офиса. Проверить страницу")
            else:
                self.publish(observed, next_action=f"Выбрать офис на сайте: доступно {len(offices)}", mode="manual")
                await self.signal(observed, "Выбрать офис", repeat=True)
                await self.page.wait_for_timeout(500)
            return
        await self.silence()
        if (observed.document, "office") in self.attempted:
            await self.hold(observed, "Переход с единственным офисом уже выполнялся. Проверить ответ")
            return
        button = await self.continuation(observed.source, r"(?:siguiente|aceptar|continuar|confirmar)")
        if button is None:
            await self.hold(observed, "Один офис, но кнопка перехода не распознана однозначно")
            return
        self.publish(observed, next_action="Выбрать единственный офис и перейти дальше")
        if not await self.guard(observed):
            return
        self.action_phase = "office.select"
        if selected_value != offices[0]["value"]:
            self.record("action", observed, "Выбрать единственный офис")
            await self.checker._select_without_focus(self.page, "#idSede, select[name='idSede']", value=offices[0]["value"])
            self.office_selected(observed, await select.input_value())
        else:
            self.record("action_completed", observed, "Единственный офис уже выбран")
        current = await self.observe()
        if current.document != observed.document or current.kind != "office" or not await self.guard(current):
            return
        await self.save(current, "STEP_BEFORE_ACTION")
        if not await self.guard(current):
            return
        self.attempted.add((observed.document, "office"))
        self.action_phase = "office.continue"
        self.record("action", current, "Перейти с единственным офисом: Siguiente")
        await self.checker._activate_control(button, timeout=5_000)
        self.last_action = "office"
        self.record("action_completed", current, "Переход с офисом отправлен")
        await self.page.wait_for_timeout(250)

    async def booking_attention(self, observed):
        if observed.kind=='booking' and not self.control.booking_handoff:
            self.control.booking_handoff=True
            self.handoff_observed=observed
            self.detach()
            self.checker._assist_guard=None
            self.pending_action=None
            self.record('handoff',observed,'Сита доступна — автоматические действия остановлены до явного перезапуска')
        self.publish(observed, mode="manual")
        if observed.kind in {"booking", "captcha"} and not self.booking_acknowledged:
            # Only trusted input counts as human attention. bring_to_front()
            # and synthetic DOM events must not silence the repeating chime.
            await self.page.evaluate("""() => {
                if (!document.documentElement.hasAttribute('data-cita-attention-installed')) {
                    document.documentElement.setAttribute('data-cita-attention-installed','1');
                    const mark = e => { if (e.isTrusted) document.documentElement.setAttribute('data-cita-human-attention','1'); };
                    document.addEventListener('pointerdown',mark,true);
                    document.addEventListener('keydown',mark,true);
                }
            }""")
            seen = await self.page.locator("html").get_attribute("data-cita-human-attention")
            if seen:
                if observed.kind=='booking':self.booking_acknowledged=True
                await self.silence()
            else:
                reason = "Пройти капчу" if observed.kind == "captcha" else "Есть сита. Требуется участие в окне сайта"
                await self.signal(observed, reason, repeat=True)
        await self.save(observed, "BOOKING_SELECTION_SCREEN" if observed.kind == "booking" else "CAPTCHA_SCREEN")
        await self.page.wait_for_timeout(500)

    async def recover_timeout(self, previous, current, exc):
        from .assist_runtime import AssistRestartRequested
        if current.kind in {'booking', 'captcha', 'unknown'}:
            await self.hold(current, 'Действие не подтверждено. Проверить страницу в Camoufox')
            return
        if previous and (current.document != previous.document or current.kind != previous.kind):
            return
        message = str(exc)
        click_phase = self.action_phase.endswith('.click') or self.action_phase in {'office.continue','expired.continue','server_error.continue'}
        pre_dispatch = (click_phase and ('waiting for element' in message or 'waiting for locator' in message)
                        and 'performing click action' not in message and 'click action done' not in message)
        key = (current.document, self.action_phase)
        attempt = self.timeout_retries.get(key, 0)
        if pre_dispatch and attempt < len(self.timings.timeout_retry_seconds):
            self.timeout_retries[key] = attempt + 1
            delay = self.timings.recovery_delay(self.timings.timeout_retry_seconds, attempt)
            if await self.countdown(current, delay, 'Повторить неотправленное действие'):
                self.attempted.discard((current.document, current.kind))
                self.pending_action = None
                self.record('interaction_retry', current, f'Клик не отправлен — повтор {attempt + 1}/{len(self.timings.timeout_retry_seconds)}')
            return
        # A dispatched POST may still be committing. Never send it twice.
        if not pre_dispatch:
            await self.page.wait_for_timeout(self.timings.post_timeout_seconds * 1000)
            after = await self.observe()
            if after.document != current.document or after.kind != current.kind:
                return
        await self.save(current, 'ACTION_TIMEOUT_RESTART')
        raise AssistRestartRequested('таймаут действия — новый проход', clear_profile=False, delay_seconds=self.timings.recovery_delay(self.timings.restart_seconds))

    def detach(self):
        self.checker._assist_action=None
        from contextlib import suppress
        listeners, self._listeners = self._listeners, []
        if hasattr(self.page, "remove_listener"):
            for event, handler in listeners:
                # Playwright/pyee may already have removed a handler. Cleanup
                # must not replace cancellation and skip browser retention.
                with suppress(KeyError):
                    self.page.remove_listener(event, handler)

    async def run(self, *, max_cycles=None):
        try:
            return await self._run(max_cycles=max_cycles)
        finally:
            self.detach()

    async def _run(self, *, max_cycles=None):
        while not self.page.is_closed():
            try:
                if not self.control.booking_handoff and max_cycles is not None and self.control.snapshot()["requests"] >= max_cycles:
                    current = await self.observe()
                    if current.kind == "request_no_slots" and not self.control.paused:
                        return 0
                await self.step()
            except Exception as exc:
                from .assist_runtime import AssistRestartRequested
                from playwright.async_api import TimeoutError as PlaywrightTimeoutError
                if isinstance(exc, AssistRestartRequested):
                    raise
                if isinstance(exc, SiteCheckError) and exc.code == "SCREEN_CHANGED":
                    await self.page.wait_for_timeout(250)
                    continue
                if self.page.is_closed() or type(exc).__name__ == "TargetClosedError":
                    raise
                if self.control.booking_handoff:
                    # Manual navigation can invalidate a passive read. Never
                    # turn that race into automatic recovery or another POST.
                    await asyncio.sleep(.2)
                    continue
                self.save_error_details(exc)
                self.record("action_interrupted", self.last_observed, f"{exc.code if isinstance(exc, SiteCheckError) else type(exc).__name__}; phase={self.action_phase}")
                log_event(log, logging.WARNING, "assist_action_interrupted", task_id=self.task.id,
                          run_id=self.run_id, error_type=type(exc).__name__, error_field=self.action_phase or None)
                try:
                    previous = self.last_observed
                    current = await self.observe()
                    if current.kind in {"expired", "server_error", "blocked", "rate_limited"}:
                        await self.recover(current)
                    elif isinstance(exc, PlaywrightTimeoutError):
                        await self.recover_timeout(previous, current, exc)
                    elif previous and current.document != previous.document:
                        continue
                    else:
                        await self.hold(current, "Не удалось выполнить действие. Проверить страницу")
                except Exception as recovery_exc:
                    if isinstance(recovery_exc, AssistRestartRequested):
                        raise
                    self.save_error_details(recovery_exc)
                    self.control.publish(screen="Проверяем страницу", evidence="Чтение страницы временно недоступно",
                                         next_action="Дождаться доступного документа", mode="waiting")
                    await asyncio.sleep(1)
        return 0
