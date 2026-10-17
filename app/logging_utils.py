"""Structured key/value logging with mandatory secret sanitizing."""

from __future__ import annotations

import json
import logging
import re
import sys
import threading
from datetime import datetime
from typing import Any, Iterable
from urllib.parse import urlsplit, urlunsplit

_TELEGRAM_TOKEN_RE = re.compile(r"(?<!\w)\d{5,12}:[A-Za-z0-9_-]{20,}(?!\w)")
_NIE_RE = re.compile(r"(?<![A-Z0-9])[XYZ]\d{7}[A-Z](?![A-Z0-9])", re.IGNORECASE)
_URL_RE = re.compile(r"https?://[^\s\"'<>]+")
_SENSITIVE_KEYS = {
    "authorization",
    "bot_token",
    "chat_id",
    "cookie",
    "contact_phone",
    "contact_email",
    "phone",
    "email",
    "csrf",
    "document_number",
    "full_name",
    "token",
}

_lock = threading.Lock()
_known_sensitive: set[str] = set()


def register_sensitive(values: Iterable[str | None]) -> None:
    """Register runtime secrets and personal values for last-resort masking."""
    with _lock:
        for value in values:
            if value and len(str(value)) >= 3:
                _known_sensitive.add(str(value))


def _strip_url_secrets(match: re.Match[str]) -> str:
    raw = match.group(0)
    trailing = ""
    while raw and raw[-1] in ".,);]":
        trailing = raw[-1] + trailing
        raw = raw[:-1]
    try:
        parts = urlsplit(raw)
        return urlunsplit((parts.scheme, parts.netloc, parts.path, "", "")) + trailing
    except ValueError:
        return "[invalid-url]" + trailing


def sanitize(value: Any) -> str:
    """Remove known secrets, Telegram tokens, NIE values and URL queries."""
    text = str(value).replace("\r", "\\r").replace("\n", "\\n")
    with _lock:
        known = sorted(_known_sensitive, key=len, reverse=True)
    for secret in known:
        text = text.replace(secret, "***")
    text = _TELEGRAM_TOKEN_RE.sub("***", text)
    text = _NIE_RE.sub("***", text)
    text = _URL_RE.sub(_strip_url_secrets, text)
    return text


def mask_chat_id(chat_id: str | int) -> str:
    value = str(chat_id)
    return "*" * max(0, len(value) - 4) + value[-4:]


def _format_value(value: Any) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, (list, tuple, set)):
        value = ",".join(str(item) for item in value)
    clean = sanitize(value)
    if not clean or any(ch.isspace() for ch in clean) or any(ch in clean for ch in '="'):
        return json.dumps(clean, ensure_ascii=False)
    return clean


class KeyValueFormatter(logging.Formatter):
    def format(self, record: logging.LogRecord) -> str:
        timestamp = datetime.fromtimestamp(record.created).astimezone().isoformat(timespec="seconds")
        event = getattr(record, "event", "log")
        parts = [timestamp, record.levelname, f"event={_format_value(event)}"]
        fields = getattr(record, "fields", {})
        for key, value in fields.items():
            safe_key = re.sub(r"[^a-zA-Z0-9_.-]", "_", str(key))
            if safe_key.lower() in _SENSITIVE_KEYS:
                value = "***"
            parts.append(f"{safe_key}={_format_value(value)}")
        message = record.getMessage()
        if message:
            parts.append(f"message={_format_value(message)}")
        if record.exc_info:
            parts.append(f"exception={_format_value(self.formatException(record.exc_info))}")
        return " ".join(parts)


def configure_logging(level: str = "INFO") -> None:
    handler = logging.StreamHandler(sys.stdout)
    handler.setFormatter(KeyValueFormatter())
    root = logging.getLogger()
    root.handlers.clear()
    root.addHandler(handler)
    root.setLevel(getattr(logging, level.upper(), logging.INFO))


def log_event(
    logger: logging.Logger,
    level: int,
    event: str,
    message: str = "",
    **fields: Any,
) -> None:
    logger.log(level, message, extra={"event": event, "fields": fields})
