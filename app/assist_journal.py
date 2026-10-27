"""Human page visits, independent of the detailed diagnostic event stream."""
from copy import deepcopy
from datetime import datetime, timedelta

STATUSES = {
    'processing': ('Обработка', 'accent'),
    'passed': ('Пройдено', 'muted'),
    'checked': ('Проверено', 'muted'),
    'waiting': ('Ожидание', 'attention'),
    'login_required': ('Нужен вход', 'attention'),
    'choice_required': ('Нужен выбор', 'attention'),
    'offices_found': ('Найдены офисы', 'success'),
    'date_available': ('Доступен выбор даты', 'success'),
    'recovering': ('Восстановление', 'attention'),
    'review_required': ('Нужна проверка', 'attention'),
    'failed': ('Не удалось продолжить', 'danger'),
    'new_pass': ('Новый проход', 'attention'),
    'interrupted': ('Прервано', 'attention'),
    'unconfirmed': ('Переход не подтверждён', 'attention'),
    'paused': ('Приостановлено', 'attention'),
    'cancelled': ('Прервано пользователем', 'muted'),
}
ERROR_STAGES = {'expired', 'server_error', 'blocked', 'rate_limited'}
NO_SLOTS = {'request_no_slots', 'request_no_slots_restart', 'post_contact_no_slots'}
EXPECTED = {
    'province': {'procedure'}, 'procedure': {'auth', 'auth_wait', 'applicant', 'applicant_no_slots'},
    'auth': {'auth_wait', 'applicant', 'applicant_no_slots'}, 'auth_wait': {'applicant', 'applicant_no_slots', 'request_menu'},
    'applicant_no_slots': {'applicant', 'applicant_no_slots'},
    'applicant': {'request_menu', 'office', 'contact', 'booking'} | NO_SLOTS,
    'request_menu': {'office', 'contact', 'booking'} | NO_SLOTS,
    'office': {'contact', 'booking'} | NO_SLOTS,
    'contact': {'booking'} | NO_SLOTS,
    'request_no_slots': {'request_menu','province','applicant','applicant_no_slots','office', 'contact', 'booking'} | NO_SLOTS,
    'request_no_slots_restart': {'province'}, 'post_contact_no_slots': {'province'},
    'captcha': {'booking', 'office', 'contact', 'applicant', 'request_menu'},
    'booking': {'contact', 'booking'},
}
ACTION_MESSAGES = {
    'province': 'Выбираем провинцию', 'procedure': 'Выбираем услугу',
    'auth': 'Начинаем авторизацию', 'auth_wait': 'Открываем выбранный способ авторизации',
    'applicant': 'Заполняем данные заявителя', 'request_menu': 'Запрашиваем ситу',
    'contact': 'Заполняем контактные данные', 'office': 'Выбираем офис',
    'request_no_slots': 'Повторяем проверку сит',
    'applicant_no_slots': 'Обновляем форму без ввода данных',
    'request_no_slots_restart': 'Возвращаемся к началу через Aceptar',
    'post_contact_no_slots': 'Завершаем проход после отсутствия сит',
}
SENT_MESSAGES = {
    'province': 'Выбор провинции отправлен — ожидается ответ',
    'procedure': 'Выбор услуги отправлен — ожидается ответ',
    'applicant': 'Данные заявителя отправлены — ожидается ответ',
    'contact': 'Контактные данные отправлены — ожидается ответ',
    'office': 'Выбранный офис отправлен — ожидается ответ',
}
PASSED_MESSAGES = {
    'province': 'Провинция выбрана → {destination}',
    'procedure': 'Услуга выбрана → {destination}',
    'auth': 'Открыта авторизация → {destination}',
    'auth_wait': 'Авторизация завершена → {destination}',
    'applicant': 'Данные заявителя отправлены → {destination}',
    'contact': 'Контактные данные отправлены → {destination}',
    'office': 'Выбран офис: {selected_office} → {destination}',
}


def stamp():
    return datetime.now().astimezone().isoformat(timespec='milliseconds')


def description(row, translate=lambda text: text, title=lambda stage: stage):
    # Only application templates are translated. Site data stays verbatim.
    values = dict(destination=translate(title(row.get('destination', ''))),
                  offices=' · '.join(row.get('offices', [])),
                  selected_office=row.get('selected_office', ''),
                  button=row.get('button',''),
                  deadline=(row.get('deadline') or '')[11:19])
    text=translate(row.get('message', '')).format_map(values)
    if row.get('stage')=='booking':
        from .booking_details import booking_lines
        lines=booking_lines(row.get('booking_details'))
        if lines:text+=' · '+' · '.join(lines)
    return text


class PageJournal:
    def __init__(self, persist):
        self.rows = []
        self.active = None
        self.persist = persist
        self.sequence = 0
        self.pass_number = 0
        self.restart_pending = False

    def snapshot(self):
        return deepcopy(self.rows)

    def update(self, **changes):
        if self.active is None:
            return
        if all(self.active.get(k) == v for k, v in changes.items()):
            return
        self.active.update(changes)
        self.persist(deepcopy(self.active))

    def add(self, **values):
        self.sequence += 1
        row = dict(id=self.sequence, time=stamp(), ended_at=None, event='visit',
                   page='', stage='', status='processing', message='', destination='',
                   offices=[], selected_office='', deadline=None, submitted=False, checked=False)
        row.update(values)
        if row['event'] != 'visit':
            row['ended_at'] = row['time']
        self.rows.append(row)
        self.rows = self.rows[-200:]
        if self.active and not self.active['ended_at'] and self.active not in self.rows:
            self.rows[0] = self.active
        self.persist(deepcopy(row))
        return row

    def observe(self, *, document, stage, page, title, applicant_continue=False):
        if stage == 'loading':
            return
        old = self.active
        if old and not old['ended_at'] and old['stage'] == stage and (
                old['document'] == document or stage == 'auth_wait'):
            # QR refresh and SSO redirects remain one authentication interaction.
            self.update(document=document, page=page, applicant_continue=bool(applicant_continue))
            return
        if old and not old['ended_at']:
            self.complete(stage)
        status = ('recovering' if stage in ERROR_STAGES else
                  'review_required' if stage == 'unknown' else
                  'date_available' if stage == 'booking' else 'processing')
        message = ('Обрабатываем сообщение об ошибке' if stage in ERROR_STAGES else
                   'Неизвестная страница — требуется ручная проверка' if stage == 'unknown' else
                   'Открыт выбор даты и времени — управление передано пользователю' if stage == 'booking' else
                   ACTION_MESSAGES.get('applicant' if stage=='applicant_no_slots' and applicant_continue else stage, 'Распознана страница — определяем следующий шаг'))
        self.active = self.add(document=document, stage=stage, page=page, title=title,
                               status=status, message=message, applicant_continue=bool(applicant_continue))
        if stage != 'province':
            self.restart_pending = False

    def complete(self, destination):
        old = self.active
        stage = old['stage']
        action_stage='applicant' if stage=='applicant_no_slots' and old.get('applicant_continue') else stage
        if old['status'] == 'failed':
            self.update(ended_at=stamp())
            return
        if stage in ERROR_STAGES and destination == stage:
            status, message = 'waiting', 'Ошибка сохраняется — требуется следующая попытка'
        elif destination in ERROR_STAGES:
            status, message = 'interrupted', 'Переход прерван сообщением сайта → {destination}'
        elif destination == 'unknown':
            status, message = 'unconfirmed', 'Результат действия не распознан → {destination}'
        elif stage in ERROR_STAGES:
            if destination in ERROR_STAGES or destination == stage:
                status, message = 'waiting', 'Ошибка сохраняется — требуется следующая попытка'
            else:
                status = 'passed'
                message = ('Кнопка {button} → {destination}' if old.get('button') else
                           'Доступ восстановлен → {destination}' if stage == 'rate_limited' else
                           'Переход после сообщения сайта → {destination}')
        elif stage in NO_SLOTS and destination == stage and old['checked']:
            status, message = 'checked', 'После обновления сит по-прежнему нет'
        elif destination in EXPECTED.get(stage, set()) | EXPECTED.get(action_stage, set()):
            status = 'offices_found' if stage == 'office' and old['offices'] else 'passed'
            message = ('Авторизация завершена → {destination}' if stage == 'auth' and destination != 'auth_wait' else
                       PASSED_MESSAGES.get(action_stage, 'Переход подтверждён → {destination}'))
        elif stage == 'booking':
            status, message = 'date_available', old['message']
        else:
            status, message = 'unconfirmed', 'Открылась другая страница → {destination}'
        self.update(status=status, message=message, destination=destination, ended_at=stamp(), deadline=None)

    def phase(self, mode, remaining, next_action):
        row = self.active
        if not row or row['ended_at'] or row['status'] in {'failed', 'paused'} or (mode == 'manual' and row['status'] == 'review_required'):
            return
        if mode == 'waiting' and remaining is not None:
            if row['status'] != 'waiting' or not row.get('deadline'):
                deadline = (datetime.now().astimezone() + timedelta(seconds=remaining)).isoformat(timespec='seconds')
                message = ('Сит нет — следующая проверка в {deadline}' if row['stage'] == 'request_no_slots' else
                           'Сит нет — новый проход после паузы до {deadline}' if row['stage'] in NO_SLOTS else
                           'Следующая попытка в {deadline}')
                self.update(status='waiting', message=message, deadline=deadline)
        elif mode == 'manual':
            stage = row['stage']
            if stage in {'auth', 'auth_wait'}:
                self.update(status='login_required', message='Открыта авторизация — требуется выполнить вход')
            elif stage == 'office' and len(row['offices']) > 1:
                self.update(status='choice_required', message='Доступно несколько офисов: {offices} — требуется выбор')
            elif stage == 'booking':
                self.update(status='date_available', message='Открыт выбор даты и времени — управление передано пользователю')
            else:
                self.update(status='review_required', message='Требуется ручная проверка страницы')
        elif mode == 'recovery':
            self.update(status='recovering', message='Восстанавливаем доступ — ожидается результат')

    def offices(self, options, selected=None):
        if self.active and self.active['stage'] == 'office' and not self.active['ended_at']:
            changes = dict(offices=[o['text'] for o in options])
            if selected:
                changes['selected_office'] = selected
            if len(options) == 1:
                changes.update(status='offices_found', message='Доступен офис: {offices}')
            self.update(**changes)

    def booking(self, document, details):
        if self.active and self.active['stage']=='booking' and self.active['document']==document:
            self.update(booking_details=deepcopy(details))

    def event(self, event, *, detail='', document=None, stage='', page='', user_requested=False):
        row = self.active
        controls = {'start', 'finish', 'pause', 'resume', 'flow_start', 'flow_restart',
                    'handoff', 'statistics_error', 'browser_shown', 'browser_show_failed'}
        if event == 'fatal_error' and (not row or row['ended_at']):
            self.add(event=event, status='failed', message=detail if detail.startswith('Browser startup failed — ') else 'Работа завершена из-за ошибки', title='Управление', ended_at=stamp(), page=page)
            return
        if event == 'start' and self.sequence:
            return
        if event in controls:
            if event in {'flow_start', 'flow_restart'} and self.restart_pending:
                if stage == 'province':
                    self.restart_pending = False
                return
            if event == 'flow_restart' and stage == 'province' and row and not row['ended_at'] and document != row['document']:
                self.complete('province')
            if event in {'flow_start', 'flow_restart'}:
                self.restart_pending = True
                self.pass_number += 1
            if event == 'pause' and row and not row['ended_at']:
                self.update(resume_status=row['status'], resume_message=row['message'],
                            status='paused', message='Автоматизация приостановлена')
            if event == 'resume' and row and row['status'] == 'paused':
                self.update(status=row.get('resume_status', 'processing'),
                            message=row.get('resume_message', 'Обработка продолжается'), deadline=None)
            if event in {'finish', 'flow_restart'} and row and not row['ended_at'] and not (event == 'flow_restart' and stage == 'province' and document == row['document']):
                if row['stage'] == 'booking':
                    self.update(status='date_available', message='Открыт выбор даты и времени — управление передано пользователю')
                elif row['status'] != 'failed':
                    self.update(status='cancelled' if event == 'finish' and user_requested else 'interrupted',
                                message='Работа прервана пользователем' if event == 'finish' and user_requested else
                                'Поиск завершён до подтверждения перехода' if event == 'finish' else 'Начинаем новый проход')
                self.update(ended_at=stamp(), deadline=None)
            messages = {'start':'Поиск запущен', 'finish':'Поиск завершён', 'pause':'Автоматизация приостановлена',
                        'resume':'Обработка продолжается', 'flow_start':'Начинаем первый проход',
                        'handoff':'Автоматизация остановлена — Camoufox доступен для ручного оформления',
                        'statistics_error':'Не удалось сохранить счётчик профиля',
                        'browser_shown':'Camoufox показан', 'browser_show_failed':'Не удалось показать Camoufox'}
            if event == 'flow_restart':
                reason = detail.casefold()
                message = ('Новый проход после истечения сессии' if any(t in reason for t in ('истёк','истек','session_expired')) else
                           'Новый проход после блокировки' if 'блокиров' in reason else
                           'Новый проход после ошибки сайта' if 'ошиб' in reason else
                           'Новый проход после таймаута действия' if 'таймаут' in reason else
                           'Новый проход после отсутствия сит' if any(t in reason for t in ('нет сит','отсутствия сит')) else
                           'Перезапуск запрошен пользователем' if any(t in reason for t in ('ручн','пользовател')) else
                           'Начинаем новый проход — причина возврата не установлена')
            else:
                message = messages[event]
            self.add(event=event, pass_number=self.pass_number, status='failed' if event == 'statistics_error' else
                     'new_pass' if event == 'flow_restart' else 'paused' if event == 'pause' else
                     'processing' if event in {'start', 'resume', 'flow_start'} else 'review_required' if event in {'handoff','browser_show_failed'} else 'passed',
                     message=message, page=page, stage='', title='Управление', ended_at=stamp())
            return
        if not row or row['ended_at'] or (document is not None and document != row['document']) or (stage and stage != row['stage']):
            return
        if event in {'action', 'action_completed', 'check', 'office_submitted'}:
            action_stage='applicant' if row['stage']=='applicant_no_slots' and row.get('applicant_continue') else row['stage']
            message = (SENT_MESSAGES.get(action_stage, 'Действие отправлено — ожидается ответ')
                       if event != 'action' else ACTION_MESSAGES.get(action_stage, 'Выполняем действие на странице'))
            if event == 'check':
                message = 'Запрос проверки сит отправлен — ожидается ответ'
            self.update(status='processing', message=message, submitted=event != 'action' or row['submitted'],
                        checked=event == 'check' or row['checked'], deadline=None)
        elif event in {'recovery', 'interaction_retry', 'action_interrupted'}:
            self.update(status='recovering', message='Восстанавливаем доступ — ожидается результат', deadline=None)
        elif event == 'manual_required':
            self.update(status='review_required', message=detail or 'Требуется ручная проверка страницы')
        elif event == 'recovery_button':
            self.update(status='recovering', message='Нажимаем {button} — ожидается результат', button=detail)
        elif event == 'fatal_error':
            self.update(status='failed', message='Действие прервано ошибкой — работа завершена', ended_at=stamp(), deadline=None)
