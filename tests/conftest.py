from __future__ import annotations

from pathlib import Path

import pytest
import yaml

from app.config import DEFAULT_START_URL, TIE_PROCEDURE


@pytest.fixture
def catalog_path() -> Path:
    return Path(__file__).resolve().parents[1] / "catalogs/snapshot-2026-08-12.json"


@pytest.fixture
def task_mapping() -> dict:
    return {
        "settings": {
            "timezone": "Europe/Madrid",
            "max_concurrency": 1,
            "min_interval_seconds": 600,
            "retry": {"attempts": 2, "backoff_seconds": [1]},
        },
        "tasks": [
            {
                "id": "client_01_tie",
                "label": "Клиент 01",
                "enabled": True,
                "request": {
                    "province": "Barcelona",
                    "office": "CNP RAMBLA GUIPUSCOA 74, RAMBLA GUIPUSCOA, 74, BARCELONA",
                    "procedure": TIE_PROCEDURE,
                    "subprocedure": None,
                },
                "applicant": {
                    "document_type": "NIE",
                    "document_number": "X0000000T",
                    "full_name": "NOMBRE APELLIDO",
                    "nationality": "UCRANIA",
                },
                "schedule": {
                    "timezone": "Europe/Madrid",
                    "days": ["mon", "tue", "wed", "thu", "fri"],
                    "windows": [{"start": "08:00", "end": "18:00", "every_seconds": 600}],
                    "jitter_seconds": [10, 45],
                },
                "notify": {
                    "chat_ids": ["123456789"],
                    "no_slots": "never",
                    "errors": "after_retries",
                    "recovered": True,
                    "on_available": "pause",
                    "open_url": DEFAULT_START_URL,
                },
            }
        ],
    }


@pytest.fixture
def config_file(tmp_path: Path, task_mapping: dict) -> Path:
    path = tmp_path / "tasks.yml"
    path.write_text(yaml.safe_dump(task_mapping, allow_unicode=True), encoding="utf-8")
    return path


@pytest.fixture(scope="session")
def qt_app():
    import os
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    pytest.importorskip("PySide6")
    from PySide6.QtWidgets import QApplication
    app = QApplication.instance() or QApplication([])
    yield app
    app.closeAllWindows()
    app.processEvents()
