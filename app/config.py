"""Configuration loading and task-level validation."""

from __future__ import annotations

import os
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

import yaml

DEFAULT_START_URL = "https://icp.administracionelectronica.gob.es/icpplus/index.html"
TIE_PROCEDURE = (
    "POLICÍA-TOMA DE HUELLAS (EXPEDICIÓN DE TARJETA) INICIAL, RENOVACIÓN, "
    "DUPLICADO Y LEY 14/2013"
)
REGRESO_PROCEDURE = "AUTORIZACIÓN DE REGRESO"
PROCEDURE_NAMES = {TIE_PROCEDURE: "Toma de huellas · TIE", REGRESO_PROCEDURE: "Autorización de regreso"}
_TASK_ID_RE = re.compile(r"^[a-z0-9_-]+$")
_NIE_RE = re.compile(r"^[XYZ]\d{7}[A-Z]$")
_NIE_LETTERS = "TRWAGMYFPDXBNJZSQVHLCKE"


class FatalConfigError(ValueError):
    """A global ambiguity that prevents safely starting any task."""


@dataclass(frozen=True)
class ScheduleConfig:
    timezone: str


@dataclass(frozen=True)
class TaskConfig:
    id: str
    label: str
    enabled: bool
    access_mode: str
    province: str
    office: str
    procedure: str
    subprocedure: str | None
    document_type: str
    document_number: str
    full_name: str
    nationality: str
    contact_phone: str
    contact_email: str
    schedule: ScheduleConfig
    errors: tuple[str, ...] = field(default_factory=tuple)
    applicant_fill_mode: str = "manual"
    clave_method: str = "movil"

    @property
    def valid(self) -> bool:
        return not self.errors


from .timings import AssistTimings


@dataclass(frozen=True)
class Settings:
    assist_timings: AssistTimings = field(default_factory=AssistTimings)
    timezone: str = "Europe/Madrid"
    navigation_timeout_seconds: int = 45
    page_reload_attempts: int = 1
    browser_preset: str = "localized"
    browser_engine: str = "webkit"
    browser_executable_path: str = ""
    browser_mode: str = "headed"
    browser_session: str = "fresh"
    browser_profile_directory: str = "data/browser-profiles"
    browser_action_delay_seconds: tuple[float, float] = (3.0, 5.0)
    browser_bootstrap_enabled: bool = True
    browser_bootstrap_wait_seconds: tuple[float, float] = (5.0, 8.0)
    browser_block_recovery_attempts: int = 1
    browser_block_recovery_delay_seconds: int = 30
    assist_alert_sound: str = ""
    diagnostics_enabled: bool = False
    diagnostics_directory: str = "debug"
    diagnostics_retention_days: int = 3


@dataclass(frozen=True)
class AppConfig:
    settings: Settings
    tasks: tuple[TaskConfig, ...]
    catalog_path: Path


def load_dotenv(path: Path) -> None:
    """Load a minimal KEY=VALUE dotenv without overriding exported variables."""
    if not path.exists():
        return
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip().strip('"').strip("'")
        if key:
            os.environ.setdefault(key, value)


def _valid_nie(value: str) -> bool:
    if not _NIE_RE.fullmatch(value):
        return False
    number = str({"X": 0, "Y": 1, "Z": 2}[value[0]]) + value[1:8]
    return _NIE_LETTERS[int(number) % 23] == value[-1]


def _load_catalog(path: Path) -> dict[str, set[str]]:
    try:
        import json

        raw = json.loads(path.read_text(encoding="utf-8"))
        selectors = raw["selectors"]
        return {
            name: {str(item["label"]) for item in data["options"]}
            for name, data in selectors.items()
        }
    except (OSError, KeyError, TypeError, ValueError) as exc:
        raise FatalConfigError(f"Cannot load catalog: {exc}") from exc


def _parse_settings(raw: dict[str, Any]) -> Settings:
    try:
        timezone = str(raw.get("timezone", "Europe/Madrid"))
        ZoneInfo(timezone)
        retry = raw.get("retry", {}) or {}
        diagnostics = raw.get("diagnostics", {}) or {}
        browser = raw.get("browser", {}) or {}
        bootstrap = browser.get("f5_bootstrap", {}) or {}
        assist = raw.get("assist", {}) or {}
        action_delay = browser.get("action_delay_seconds", [3.0, 5.0])
        bootstrap_wait = bootstrap.get("wait_seconds", [5.0, 8.0])
        settings = Settings(
            timezone=timezone,
            navigation_timeout_seconds=int(raw.get("navigation_timeout_seconds", 45)),
            page_reload_attempts=int(retry.get("page_reload_attempts", 1)),
            browser_preset=str(browser.get("preset", "localized")).lower(),
            browser_engine=str(
                os.environ.get("BROWSER_ENGINE", browser.get("engine", "webkit"))
            ).lower(),
            browser_executable_path=str(
                os.environ.get(
                    "BROWSER_EXECUTABLE_PATH",
                    browser.get("executable_path", ""),
                )
            ).strip(),
            browser_mode=str(os.environ.get("BROWSER_MODE", browser.get("mode", "headed"))).lower(),
            browser_session=str(
                os.environ.get("BROWSER_SESSION", browser.get("session", "fresh"))
            ).lower(),
            browser_profile_directory=str(
                os.environ.get(
                    "BROWSER_PROFILE_DIR",
                    browser.get("profile_directory", "data/browser-profiles"),
                )
            ),
            browser_action_delay_seconds=(float(action_delay[0]), float(action_delay[1])),
            browser_bootstrap_enabled=bool(bootstrap.get("enabled", True)),
            browser_bootstrap_wait_seconds=(
                float(bootstrap_wait[0]),
                float(bootstrap_wait[1]),
            ),
            browser_block_recovery_attempts=int(bootstrap.get("block_recovery_attempts", 1)),
            browser_block_recovery_delay_seconds=int(
                bootstrap.get("block_recovery_delay_seconds", 30)
            ),
            assist_alert_sound=str(assist.get("alert_sound", "")),
            diagnostics_enabled=bool(diagnostics.get("enabled", False)),
            diagnostics_directory=str(diagnostics.get("directory", "debug")),
            diagnostics_retention_days=int(diagnostics.get("retention_days", 3)),
        )
    except (IndexError, TypeError, ValueError, ZoneInfoNotFoundError) as exc:
        raise FatalConfigError(f"Invalid global settings: {exc}") from exc
    if not 0 <= settings.page_reload_attempts <= 3:
        raise FatalConfigError("settings.retry.page_reload_attempts must be between 0 and 3")
    if settings.navigation_timeout_seconds < 5:
        raise FatalConfigError("navigation_timeout_seconds must be >= 5")
    if settings.browser_mode not in {"headed", "headless"}:
        raise FatalConfigError("settings.browser.mode must be headed or headless")
    if settings.browser_preset not in {"localized", "vanilla"}:
        raise FatalConfigError("settings.browser.preset must be localized or vanilla")
    if settings.browser_engine not in {"chromium", "webkit", "camoufox"}:
        raise FatalConfigError("settings.browser.engine must be chromium, webkit or camoufox")
    if settings.browser_executable_path and settings.browser_engine != "chromium":
        raise FatalConfigError("settings.browser.executable_path requires browser.engine=chromium")
    if settings.browser_session not in {"fresh", "persistent"}:
        raise FatalConfigError("settings.browser.session must be fresh or persistent")
    if not settings.browser_profile_directory.strip():
        raise FatalConfigError("settings.browser.profile_directory must not be empty")
    if (
        settings.browser_action_delay_seconds[0] < 0
        or settings.browser_action_delay_seconds[1] < settings.browser_action_delay_seconds[0]
        or settings.browser_action_delay_seconds[1] > 10
    ):
        raise FatalConfigError("settings.browser.action_delay_seconds must be [min,max], 0 <= min <= max <= 10")
    if (
        settings.browser_bootstrap_wait_seconds[0] < 0
        or settings.browser_bootstrap_wait_seconds[1] < settings.browser_bootstrap_wait_seconds[0]
        or settings.browser_bootstrap_wait_seconds[1] > 30
    ):
        raise FatalConfigError("settings.browser.f5_bootstrap.wait_seconds must be [min,max], 0 <= min <= max <= 30")
    if not 0 <= settings.browser_block_recovery_attempts <= 1:
        raise FatalConfigError("settings.browser.f5_bootstrap.block_recovery_attempts must be 0 or 1")
    if settings.browser_block_recovery_delay_seconds < 10:
        raise FatalConfigError("settings.browser.f5_bootstrap.block_recovery_delay_seconds must be >= 10")
    return settings


def _task(raw: dict[str, Any], settings: Settings, catalog: dict[str, set[str]]) -> TaskConfig:
    errors: list[str] = []
    task_id = str(raw.get("id", ""))
    label = str(raw.get("label", task_id))
    enabled = bool(raw.get("enabled", True))
    request = raw.get("request", {}) or {}
    applicant = raw.get("applicant", {}) or {}
    schedule_raw = raw.get("schedule", {}) or {}

    access_mode = str(request.get("access_mode", "without_clave")).lower()
    clave_method = str(request.get("clave_method", "movil")).lower()
    province = str(request.get("province", ""))
    office = str(request.get("office", ""))
    procedure = str(request.get("procedure", ""))
    subprocedure = request.get("subprocedure")
    applicant_fill_mode = str(applicant.get("fill_mode", "manual")).lower()
    document_type = str(applicant.get("document_type", "")).upper()
    document_number = str(applicant.get("document_number", "")).upper().replace(" ", "")
    full_name = str(applicant.get("full_name", "")).strip()
    nationality = str(applicant.get("nationality", ""))
    contact = applicant.get("contact", {}) or {}
    contact_phone = str(contact.get("phone", "")).strip().replace(" ", "")
    contact_email = str(contact.get("email", "")).strip()

    if not _TASK_ID_RE.fullmatch(task_id):
        errors.append("id must match [a-z0-9_-]+")
    if not label:
        errors.append("label is required")
    if clave_method not in {"movil", "permanente"}:
        errors.append("request.clave_method must be movil or permanente")
    if access_mode not in {"without_clave", "clave", "certificate"}:
        errors.append("request.access_mode must be without_clave, clave or certificate")
    if access_mode in {"clave", "certificate"} and settings.browser_session != "persistent":
        errors.append(f"request.access_mode={access_mode} requires settings.browser.session=persistent")
    if access_mode == "certificate":
        if not re.fullmatch(r"\d{6,15}", contact_phone):
            errors.append("applicant.contact.phone must be 6-15 digits when access_mode=certificate")
        if not re.fullmatch(r"[^@\s]+@[^@\s]+\.[^@\s]+", contact_email):
            errors.append("applicant.contact.email must be a valid address when access_mode=certificate")
    if province not in catalog.get("provinces", set()):
        errors.append("request.province is not present in catalog")
    if office and province == "Barcelona" and office not in catalog.get("barcelona_offices", set()):
        errors.append("request.office is not present in Barcelona catalog")
    if procedure not in PROCEDURE_NAMES:
        errors.append("request.procedure must be TIE fingerprints or autorización de regreso")
    if subprocedure not in (None, ""):
        errors.append("request.subprocedure must be null in the supported flows")
    if procedure == REGRESO_PROCEDURE:
        if document_type not in {"NIE", "DNI", "PASSPORT"}:
            errors.append("applicant.document_type must be NIE, DNI or PASSPORT for regreso")
    elif document_type != "NIE":
        errors.append("applicant.document_type must be NIE")
    if applicant_fill_mode not in {"manual", "copy"}:
        errors.append("applicant.fill_mode must be manual or copy")
    if applicant_fill_mode == "copy" and access_mode not in {"clave", "certificate"}:
        errors.append("applicant.fill_mode=copy requires certificate or Cl@ve")
    if applicant_fill_mode != "copy":
        if procedure == REGRESO_PROCEDURE and document_type == "DNI":
            if not (re.fullmatch(r"[0-9]{8}[A-Z]", document_number)
                    and _NIE_LETTERS[int(document_number[:8]) % 23] == document_number[-1]):
                errors.append("applicant.document_number is not a valid DNI")
        elif procedure == REGRESO_PROCEDURE and document_type == "PASSPORT":
            if not re.fullmatch(r"[A-Z0-9]{1,20}", document_number):
                errors.append("applicant.document_number must be a passport number of 1-20 letters or digits")
        elif not _valid_nie(document_number):
            errors.append("applicant.document_number is not a valid NIE")
    if applicant_fill_mode != "copy" and (not full_name or len(full_name) > 50 or any(ch.isdigit() for ch in full_name)):
        errors.append("applicant.full_name must be 1-50 characters without digits")
    if procedure != REGRESO_PROCEDURE and nationality not in catalog.get("nationalities", set()):
        errors.append("applicant.nationality is not present in catalog")

    timezone = str(schedule_raw.get("timezone", settings.timezone))
    try:
        ZoneInfo(timezone)
    except ZoneInfoNotFoundError:
        errors.append("schedule.timezone is invalid")
    return TaskConfig(
        id=task_id,
        label=label,
        enabled=enabled,
        access_mode=access_mode,
        clave_method=clave_method,
        applicant_fill_mode=applicant_fill_mode,
        province=province,
        office=office,
        procedure=procedure,
        subprocedure=str(subprocedure) if subprocedure else None,
        document_type=document_type,
        document_number=document_number,
        full_name=full_name,
        nationality=nationality,
        contact_phone=contact_phone,
        contact_email=contact_email,
        schedule=ScheduleConfig(timezone=timezone),
        errors=tuple(errors),
    )


def load_config(
    path: Path,
    catalog_path: Path,
    *,
    allow_missing_notify: bool = False,
) -> AppConfig:
    try:
        raw = yaml.safe_load(path.read_text(encoding="utf-8")) or {}
    except (OSError, yaml.YAMLError) as exc:
        raise FatalConfigError(f"Cannot load tasks YAML: {exc}") from exc
    if not isinstance(raw, dict) or not isinstance(raw.get("tasks", []), list):
        raise FatalConfigError("tasks.yml must contain a tasks list")
    settings = _parse_settings(raw.get("settings", {}) or {})
    catalog = _load_catalog(catalog_path)
    tasks = tuple(_task(item or {}, settings, catalog) for item in raw.get("tasks", []))
    ids = [task.id for task in tasks]
    duplicates = sorted({task_id for task_id in ids if ids.count(task_id) > 1})
    if duplicates:
        raise FatalConfigError(f"Duplicate task id: {','.join(duplicates)}")
    return AppConfig(settings=settings, tasks=tasks, catalog_path=catalog_path)
