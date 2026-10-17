from __future__ import annotations

from pathlib import Path

import pytest
import yaml

from app.config import FatalConfigError, load_config


def test_valid_task(monkeypatch, config_file: Path, catalog_path: Path):
    config = load_config(config_file, catalog_path)
    assert len(config.tasks) == 1
    assert config.tasks[0].valid
    assert config.tasks[0].access_mode == "without_clave"
    assert config.tasks[0].schedule.timezone == "Europe/Madrid"
    assert config.settings.browser_engine == "webkit"
    assert config.settings.browser_preset == "localized"
    assert config.settings.browser_executable_path == ""
    assert config.settings.browser_mode == "headed"
    assert config.settings.browser_session == "fresh"
    assert config.settings.page_reload_attempts == 1
    assert config.settings.browser_action_delay_seconds == (3.0, 5.0)
    assert config.settings.browser_bootstrap_enabled is True
    assert config.settings.browser_bootstrap_wait_seconds == (5.0, 8.0)
    assert config.settings.browser_block_recovery_attempts == 1
    assert config.settings.browser_block_recovery_delay_seconds == 30


def test_invalid_task_is_isolated(monkeypatch, config_file: Path, catalog_path: Path):
    raw = yaml.safe_load(config_file.read_text(encoding="utf-8"))
    raw["tasks"][0]["request"]["office"] = "UNKNOWN OFFICE"
    config_file.write_text(yaml.safe_dump(raw, allow_unicode=True), encoding="utf-8")
    config = load_config(config_file, catalog_path)
    assert not config.tasks[0].valid
    assert any("request.office" in error for error in config.tasks[0].errors)


def test_duplicate_ids_are_fatal(monkeypatch, config_file: Path, catalog_path: Path):
    raw = yaml.safe_load(config_file.read_text(encoding="utf-8"))
    raw["tasks"].append(dict(raw["tasks"][0]))
    config_file.write_text(yaml.safe_dump(raw, allow_unicode=True), encoding="utf-8")
    with pytest.raises(FatalConfigError, match="Duplicate task id"):
        load_config(config_file, catalog_path)








def test_invalid_browser_mode_is_fatal(config_file: Path, catalog_path: Path, monkeypatch):
    raw = yaml.safe_load(config_file.read_text(encoding="utf-8"))
    raw["settings"]["browser"] = {"mode": "stealth"}
    config_file.write_text(yaml.safe_dump(raw, allow_unicode=True), encoding="utf-8")
    with pytest.raises(FatalConfigError, match="browser.mode"):
        load_config(config_file, catalog_path)


def test_invalid_browser_engine_is_fatal(config_file: Path, catalog_path: Path, monkeypatch):
    raw = yaml.safe_load(config_file.read_text(encoding="utf-8"))
    raw["settings"]["browser"] = {"engine": "safari"}
    config_file.write_text(yaml.safe_dump(raw, allow_unicode=True), encoding="utf-8")
    with pytest.raises(FatalConfigError, match="browser.engine"):
        load_config(config_file, catalog_path)


def test_invalid_browser_preset_is_fatal(config_file: Path, catalog_path: Path, monkeypatch):
    raw = yaml.safe_load(config_file.read_text(encoding="utf-8"))
    raw["settings"]["browser"] = {"preset": "stealth"}
    config_file.write_text(yaml.safe_dump(raw, allow_unicode=True), encoding="utf-8")
    with pytest.raises(FatalConfigError, match="browser.preset"):
        load_config(config_file, catalog_path)


def test_custom_executable_requires_chromium(config_file: Path, catalog_path: Path, monkeypatch):
    raw = yaml.safe_load(config_file.read_text(encoding="utf-8"))
    raw["settings"]["browser"] = {
        "engine": "webkit",
        "executable_path": "/Applications/Some Browser.app/browser",
    }
    config_file.write_text(yaml.safe_dump(raw, allow_unicode=True), encoding="utf-8")
    with pytest.raises(FatalConfigError, match="executable_path"):
        load_config(config_file, catalog_path)


def test_invalid_browser_session_is_fatal(config_file: Path, catalog_path: Path, monkeypatch):
    raw = yaml.safe_load(config_file.read_text(encoding="utf-8"))
    raw["settings"]["browser"] = {"session": "shared"}
    config_file.write_text(yaml.safe_dump(raw, allow_unicode=True), encoding="utf-8")
    with pytest.raises(FatalConfigError, match="browser.session"):
        load_config(config_file, catalog_path)


def test_clave_access_requires_persistent_session(
    config_file: Path, catalog_path: Path, monkeypatch
):
    raw = yaml.safe_load(config_file.read_text(encoding="utf-8"))
    raw["tasks"][0]["request"]["access_mode"] = "clave"
    config_file.write_text(yaml.safe_dump(raw, allow_unicode=True), encoding="utf-8")

    config = load_config(config_file, catalog_path)

    assert not config.tasks[0].valid
    assert any("browser.session=persistent" in error for error in config.tasks[0].errors)


def test_clave_access_is_valid_with_persistent_session(
    config_file: Path, catalog_path: Path, monkeypatch
):
    raw = yaml.safe_load(config_file.read_text(encoding="utf-8"))
    raw["settings"]["browser"] = {"session": "persistent"}
    raw["tasks"][0]["request"]["access_mode"] = "clave"
    config_file.write_text(yaml.safe_dump(raw, allow_unicode=True), encoding="utf-8")

    config = load_config(config_file, catalog_path)

    assert config.tasks[0].valid
    assert config.tasks[0].access_mode == "clave"


def test_invalid_access_mode_is_task_error(config_file: Path, catalog_path: Path, monkeypatch):
    raw = yaml.safe_load(config_file.read_text(encoding="utf-8"))
    raw["tasks"][0]["request"]["access_mode"] = "magic"
    config_file.write_text(yaml.safe_dump(raw, allow_unicode=True), encoding="utf-8")

    config = load_config(config_file, catalog_path)

    assert not config.tasks[0].valid
    assert any("request.access_mode" in error for error in config.tasks[0].errors)


def test_invalid_browser_delay_is_fatal(config_file: Path, catalog_path: Path, monkeypatch):
    raw = yaml.safe_load(config_file.read_text(encoding="utf-8"))
    raw["settings"]["browser"] = {"action_delay_seconds": [3, 1]}
    config_file.write_text(yaml.safe_dump(raw, allow_unicode=True), encoding="utf-8")
    with pytest.raises(FatalConfigError, match="action_delay_seconds"):
        load_config(config_file, catalog_path)


def test_invalid_page_reload_attempts_is_fatal(config_file: Path, catalog_path: Path, monkeypatch):
    raw = yaml.safe_load(config_file.read_text(encoding="utf-8"))
    raw["settings"]["retry"]["page_reload_attempts"] = 4
    config_file.write_text(yaml.safe_dump(raw, allow_unicode=True), encoding="utf-8")
    with pytest.raises(FatalConfigError, match="page_reload_attempts"):
        load_config(config_file, catalog_path)


def test_invalid_f5_bootstrap_recovery_is_fatal(config_file: Path, catalog_path: Path, monkeypatch):
    raw = yaml.safe_load(config_file.read_text(encoding="utf-8"))
    raw["settings"]["browser"] = {
        "f5_bootstrap": {"block_recovery_attempts": 2}
    }
    config_file.write_text(yaml.safe_dump(raw, allow_unicode=True), encoding="utf-8")
    with pytest.raises(FatalConfigError, match="block_recovery_attempts"):
        load_config(config_file, catalog_path)


def test_diagnostics_default_off(config_file, catalog_path):
    from app.config import Settings
    assert Settings().diagnostics_enabled is False
    raw = yaml.safe_load(config_file.read_text(encoding='utf-8'))
    raw['settings'].pop('diagnostics', None)
    config_file.write_text(yaml.safe_dump(raw), encoding='utf-8')
    assert load_config(config_file, catalog_path).settings.diagnostics_enabled is False
    raw['settings']['diagnostics'] = {'enabled': True}
    config_file.write_text(yaml.safe_dump(raw), encoding='utf-8')
    assert load_config(config_file, catalog_path).settings.diagnostics_enabled is True
