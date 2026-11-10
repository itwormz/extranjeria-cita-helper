"""Persist budget state without storing the applicant document."""
import hashlib
import hmac
import json
import os
import secrets
import re
import tempfile
from dataclasses import asdict
from datetime import date, datetime
from pathlib import Path

from .appointment_budget import AppointmentBudget
from .file_replace import replace_file
from .locking import locked_file


class BudgetStore:
    def __init__(self, root: Path, task, *, use_saved_profile=False):
        self.root = root
        root.mkdir(parents=True, exist_ok=True)
        secret = root / 'key'
        with locked_file(root / '.key.lock'):
            if not secret.exists():
                fd = os.open(secret, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
                with os.fdopen(fd, 'wb') as stream:
                    stream.write(secrets.token_bytes(32))
            secret_key = secret.read_bytes()
        document = ('' if use_saved_profile and getattr(task, 'applicant_fill_mode', None) == 'copy'
                    else ''.join(task.document_number.upper().split()))
        identity = (f'{task.document_type.upper()}:{document}' if document
                    else f'profile:{task.id}')
        self.key = hmac.new(secret_key, identity.encode(), hashlib.sha256).hexdigest()
        if use_saved_profile:
            self.key = self._profile_key(root, task.id) or self.key
        self.path = root / (self.key + '.json')

    def remember_profile(self, profile_id):
        self._write(self.root / f'profile-{profile_id}.json', {'key': self.key})

    @staticmethod
    def _profile_key(root, profile_id):
        try:
            key = json.loads((root / f'profile-{profile_id}.json').read_text(encoding='utf-8'))['key']
        except FileNotFoundError:
            return None
        if not isinstance(key, str) or not re.fullmatch(r'[0-9a-f]{64}', key):
            raise ValueError('Invalid appointment budget key')
        return key

    @classmethod
    def load_profile(cls, root, profile_id):
        key = cls._profile_key(root, profile_id)
        return None if key is None else cls._load(root / (key + '.json'))

    @classmethod
    def save_profile(cls, root, profile_id, budget):
        key = cls._profile_key(root, profile_id)
        if key is None:
            return False
        cls._write_path(root, root / (key + '.json'), asdict(budget))
        return True

    def load(self):
        return self._load(self.path)

    @staticmethod
    def _load(path):
        try:
            data = json.loads(path.read_text(encoding='utf-8'))
        except FileNotFoundError:
            return AppointmentBudget()
        if data.get('day'):
            data['day'] = date.fromisoformat(data['day'])
        for key in ('last_submission', 'first_cod_oper', 'last_cod_oper', 'restriction_until'):
            if data.get(key):
                data[key] = datetime.fromisoformat(data[key])
        data['windows'] = tuple(tuple(datetime.fromisoformat(v) for v in pair)
                                for pair in data.get('windows', ()))
        if data.get('target_override') is not None and data['target_override'] < 5:
            data['target_override'] = 5
            data['windows'] = ()
        if data.get('next_target') is not None:
            data['next_target'] = max(5, data['next_target'])
        return AppointmentBudget(**data)

    def save(self, budget):
        self._write(self.path, asdict(budget))

    def _write(self, path, data):
        self._write_path(self.root, path, data)

    @staticmethod
    def _write_path(root, path, data):
        fd, temporary = tempfile.mkstemp(dir=root)
        try:
            with os.fdopen(fd, 'w', encoding='utf-8') as stream:
                json.dump(data, stream, default=lambda value: value.isoformat())
            replace_file(temporary, path)
        finally:
            Path(temporary).unlink(missing_ok=True)
