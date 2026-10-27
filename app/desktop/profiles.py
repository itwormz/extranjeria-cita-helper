"""Private profile storage with conflict detection and atomic writes."""
import copy
import hashlib
import os
import tempfile
import uuid
from dataclasses import replace
from pathlib import Path

def system_language():
    from PySide6.QtCore import QLocale
    language=QLocale.system().name().split('_')[0]
    return language if language in {'ru','en','es'} else 'en'


import yaml

from ..config import FatalConfigError, _load_catalog, _parse_settings, _task


class ProfileValidationError(ValueError):
    pass


class ProfileConflictError(ValueError):
    pass


class ProfileStore:
    def __init__(self, path: Path, catalog_path: Path):
        self.path, self.catalog_path = Path(path), Path(catalog_path)

    def revision(self):
        try:
            data = self.path.read_bytes()
        except FileNotFoundError:
            data = b''
        return hashlib.sha256(data).hexdigest()

    def _read(self):
        try:
            raw = yaml.safe_load(self.path.read_text(encoding='utf-8')) if self.path.exists() else {}
            raw = raw or {}
            if not isinstance(raw, dict) or not isinstance(raw.get('tasks', []), list):
                raise ValueError()
            if any(raw.get(key) is not None and not isinstance(raw[key], dict) for key in ('settings','desktop')):
                raise ValueError()
            if any(not isinstance(t, dict) for t in raw.get('tasks', [])):
                raise ValueError()
            return raw
        except (OSError, ValueError, yaml.YAMLError):
            raise ProfileValidationError('Не удалось прочитать файл профилей. Файл не изменён.') from None

    @staticmethod
    def timings(raw):
        from ..timings import parse_timings
        try:
            return parse_timings(raw.get('timings'))
        except (TypeError, ValueError, AttributeError) as exc:
            raise ProfileValidationError(str(exc)) from None

    @classmethod
    def action_timings(cls, raw):
        return cls.timings(raw).action_delay_seconds

    def settings(self, profile_id=None):
        try:
            settings = replace(_parse_settings(self._read().get('settings', {}) or {}),
                           browser_engine='camoufox', browser_preset='camoufox',
                           browser_session='persistent', browser_mode='headed')
        except (FatalConfigError, TypeError, ValueError):
            raise ProfileValidationError('Некорректные общие настройки в файле профилей.') from None
        if profile_id is not None:
            timings = self.timings(self.raw_profile(profile_id))
            settings = replace(settings, assist_timings=timings, browser_action_delay_seconds=timings.action_delay_seconds,
                               browser_bootstrap_wait_seconds=timings.bootstrap_seconds, navigation_timeout_seconds=timings.navigation_seconds)
        return settings

    def _convert(self, raw):
        try:
            task = _task(raw, self.settings(), _load_catalog(self.catalog_path))
            errors = list(task.errors)
            try:
                self.action_timings(raw)
            except ProfileValidationError as exc:
                errors.append(str(exc))
            if task.access_mode in {'certificate', 'clave'}:
                import re
                if not re.fullmatch(r'\d{6,15}', task.contact_phone):
                    errors.append('applicant.contact.phone: требуется телефон из 6–15 цифр')
                if not re.fullmatch(r'[^@\s]+@[^@\s]+\.[^@\s]+', task.contact_email):
                    errors.append('applicant.contact.email: требуется адрес электронной почты')
            return replace(task, errors=tuple(dict.fromkeys(errors)))
        except (TypeError, ValueError, KeyError, AttributeError):
            raise ProfileValidationError('Некорректная структура профиля.') from None

    def load(self):
        tasks = [self._convert(t) for t in self._read().get('tasks', [])]
        if len({t.id for t in tasks}) != len(tasks):
            raise ProfileValidationError('В файле повторяются идентификаторы профилей.')
        return tasks

    def raw_profile(self, profile_id):
        return copy.deepcopy(next(t for t in self._read().get('tasks', []) if t.get('id') == profile_id))

    def _write(self, raw, revision):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        # Serialize cooperating editors, then compare content before replace.
        from ..locking import locked_file
        with locked_file(self.path.with_name('.' + self.path.name + '.lock')):
            if self.revision() != revision:
                raise ProfileConflictError('Файл изменён извне. Перед сохранением открыть настройки заново.')
            fd, temp = tempfile.mkstemp(prefix='.profiles-', dir=self.path.parent)
            try:
                with os.fdopen(fd, 'w', encoding='utf-8') as stream:
                    yaml.safe_dump(raw, stream, allow_unicode=True, sort_keys=False)
                    stream.flush()
                    os.fsync(stream.fileno())
                os.replace(temp, self.path)
            finally:
                if os.path.exists(temp):
                    os.unlink(temp)

    def save_profile(self, profile_id, fields, *, revision):
        raw = self._read()
        tasks = raw.setdefault('tasks', [])
        existing = next((t for t in tasks if t.get('id') == profile_id), None)
        if profile_id and existing is None:
            raise ProfileConflictError('Профиль больше не существует. Обнови список.')
        task = copy.deepcopy(existing or {})
        profile_id = profile_id or 'person_' + uuid.uuid4().hex[:12]
        for key in ('label', 'request', 'applicant', 'timings', 'notifications'):
            if key in fields:
                if isinstance(fields[key], dict):
                    task.setdefault(key, {}).update(copy.deepcopy(fields[key]))
                else:
                    task[key] = fields[key]
        task['id'] = profile_id
        task.setdefault('enabled', True)
        candidate = self._convert(task)
        errors = list(candidate.errors)
        if candidate.access_mode not in {'certificate', 'clave'}:
            errors.append('request.access_mode: выбрать сертификат или Cl@ve')
        import re
        if not re.fullmatch(r'\d{6,15}', candidate.contact_phone):
            errors.append('applicant.contact.phone: требуется телефон из 6–15 цифр')
        if not re.fullmatch(r'[^@\s]+@[^@\s]+\.[^@\s]+', candidate.contact_email):
            errors.append('applicant.contact.email: требуется адрес электронной почты')
        if errors:
            raise ProfileValidationError('\n'.join(errors))
        if existing is None:
            tasks.append(task)
        else:
            tasks[tasks.index(existing)] = task
        self._write(raw, revision)
        return profile_id

    def last_profile(self):
        value=(self._read().get('desktop') or {}).get('last_profile_id')
        return value if isinstance(value,str) else None

    def save_last_profile(self, profile_id):
        revision=self.revision()
        raw=self._read()
        desktop=raw.setdefault('desktop',{})
        if desktop is None:desktop=raw['desktop']={}
        if desktop.get('last_profile_id')==profile_id:return
        desktop['last_profile_id']=profile_id
        self._write(raw,revision)

    def preferences(self):
        raw = self._read()
        desktop = raw.get('desktop', {}) or {}
        diagnostics = ((raw.get('settings') or {}).get('diagnostics', {}) or {}).get('enabled', False)
        return dict(dark=bool(desktop.get('dark', False)),
                    diagnostics=bool(desktop.get('diagnostics', diagnostics)), sound=bool(desktop.get('sound', True)),
                    language=desktop.get('language') if desktop.get('language') in {'ru','en','es'} else system_language(),
                    automatic_updates=bool(desktop.get('automatic_updates',True)))

    def save_preferences(self, *, dark, diagnostics, sound, language=None, automatic_updates=None):
        revision = self.revision()
        raw = self._read()
        language=language or (raw.get('desktop') or {}).get('language','en')
        if language not in {'ru','en','es'}:
            raise ProfileValidationError('Unsupported interface language')
        if automatic_updates is None:
            automatic_updates=(raw.get('desktop') or {}).get('automatic_updates',True)
        desktop=dict(raw.get('desktop') or {})
        desktop.update(dark=bool(dark), diagnostics=bool(diagnostics), sound=bool(sound),language=language,automatic_updates=bool(automatic_updates))
        raw['desktop']=desktop
        self._write(raw, revision)
