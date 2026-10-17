"""Conservative classification of the terminal appointment page."""

from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum


class Result(StrEnum):
    AVAILABLE = "AVAILABLE"
    NO_SLOTS = "NO_SLOTS"
    ERROR = "ERROR"


@dataclass(frozen=True)
class Classification:
    result: Result
    error_code: str | None = None


NO_SLOT_MARKERS = (
    "en este momento no hay citas disponibles",
    "no hay citas disponibles",
    "no existen citas disponibles",
)

CAPTCHA_MARKERS = (
    "captcha",
    "código de la imagen",
    "codigo de la imagen",
)

BLOCK_MARKERS = (
    "access denied",
    "request rejected",
    "forbidden",
    "demasiadas solicitudes",
    "too many requests",
)

AVAILABLE_SELECTORS = (
    "#idSede",
    "select[name='idSede']",
    "input[name*='fecha']",
    "select[name*='fecha']",
    "input[name*='hora']",
    "select[name*='hora']",
    ".ui-datepicker",
    "input[type='radio'][name*='cita']",
)


def classify_terminal(
    html: str,
    present_selectors: set[str] | None = None,
    *,
    visible_text: str | None = None,
) -> Classification:
    """Classify only after Solicitar Cita; intermediate pages must not call this."""
    normalized = " ".join((visible_text if visible_text is not None else html).lower().split())
    if any(marker in normalized for marker in BLOCK_MARKERS):
        return Classification(Result.ERROR, "HTTP_BLOCKED")
    if any(marker in normalized for marker in CAPTCHA_MARKERS):
        return Classification(Result.ERROR, "CAPTCHA_DETECTED")
    if any(marker in normalized for marker in NO_SLOT_MARKERS):
        return Classification(Result.NO_SLOTS)
    if present_selectors and any(selector in present_selectors for selector in AVAILABLE_SELECTORS):
        return Classification(Result.AVAILABLE)
    return Classification(Result.ERROR, "UNKNOWN_RESULT")
