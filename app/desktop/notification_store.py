"""Explicit native credential backends. No file or plaintext fallback."""
import json
import base64
import sys
import uuid
from ..notifications import NotificationError


class NotificationVault:
    SERVICE='Cita Extranjeria Helper notifications'

    def __init__(self,backend=None):self._backend=backend

    def backend(self):
        if self._backend is None:
            if sys.platform=='darwin':
                from keyring.backends.macOS import Keyring
                self._backend=Keyring()
            elif sys.platform=='win32':
                from keyring.backends.Windows import WinVaultKeyring
                self._backend=WinVaultKeyring()
            else:
                from keyring.backends.SecretService import Keyring
                self._backend=Keyring()
        return self._backend

    def put(self,data):
        reference='notifications-'+uuid.uuid4().hex
        encoded=base64.b64encode(json.dumps(data,ensure_ascii=False).encode()).decode('ascii')
        chunks=[encoded[i:i+1000] for i in range(0,len(encoded),1000)]
        if len(chunks)>256:raise NotificationError('Слишком большая конфигурация уведомлений')
        written=[]
        try:
            for index,chunk in enumerate(chunks):
                key=f'{reference}-{index}';self.backend().set_password(self.SERVICE,key,chunk);written.append(key)
            self.backend().set_password(self.SERVICE,reference,json.dumps({'_ceh_chunks':len(chunks)}))
        except Exception:
            for key in written:
                try:self.backend().delete_password(self.SERVICE,key)
                except Exception:pass
            raise NotificationError('Системное хранилище секретов недоступно. Настройки не сохранены.') from None
        return reference

    def get(self,reference):
        if not reference:return {}
        try:
            value=self.backend().get_password(self.SERVICE,reference)
            if value is None:raise ValueError()
            result=json.loads(value)
            if not isinstance(result,dict):raise ValueError()
            if '_ceh_chunks' in result:
                count=result['_ceh_chunks']
                if not isinstance(count,int) or not 1<=count<=256:raise ValueError()
                encoded=''.join(self.backend().get_password(self.SERVICE,f'{reference}-{i}') for i in range(count))
                result=json.loads(base64.b64decode(encoded,validate=True))
                if not isinstance(result,dict):raise ValueError()
            return result
        except Exception:raise NotificationError('Не удалось прочитать секреты уведомлений. Проверить системное хранилище.') from None

    def delete(self,reference):
        if not reference:return
        try:
            manifest=json.loads(self.backend().get_password(self.SERVICE,reference) or '{}')
            count=manifest.get('_ceh_chunks',0)
            if not isinstance(count,int) or not 0<=count<=256:return
            for index in range(count):self.backend().delete_password(self.SERVICE,f'{reference}-{index}')
            self.backend().delete_password(self.SERVICE,reference)
        except Exception:pass  # Orphaned native entry is safer than losing a saved profile.
