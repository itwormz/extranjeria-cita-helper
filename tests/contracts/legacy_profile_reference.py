"""Actual desktop ProfileStore against a chosen synthetic temporary file."""
import hashlib,os
from unittest.mock import patch
from app.desktop.profiles import ProfileStore,ProfileValidationError
from app.config import TIE_PROCEDURE,REGRESO_PROCEDURE

def replay(source,path,catalog,system_language='en'):
    data=source.encode('utf-8');path.write_bytes(data)
    store=ProfileStore(path,path.parent/'unused-public-catalog.json')
    try:
        with patch.dict(os.environ,{},clear=True),patch('app.desktop.profiles._load_catalog',return_value={k:set(v) for k,v in catalog.items()}),patch('app.desktop.profiles.system_language',return_value=system_language):
            tasks=store.load();settings=store.settings()
            profiles=[]
            for t in tasks:
                raw=store.raw_profile(t.id)
                profiles.append(dict(normalized=dict(id=t.id,label=t.label,enabled=t.enabled,procedure={TIE_PROCEDURE:'huellas',REGRESO_PROCEDURE:'regreso'}.get(t.procedure,t.procedure),province=t.province,office=t.office,document_type=t.document_type,document_number=t.document_number,full_name=t.full_name,nationality=t.nationality,phone=t.contact_phone,email=t.contact_email,access_mode=t.access_mode,clave_method=t.clave_method,fill_mode=t.applicant_fill_mode,no_slots_mode=t.no_slots_mode,applicant_no_slots_mode=t.applicant_no_slots_mode,subprocedure=t.subprocedure,timezone=t.schedule.timezone,browser_session=settings.browser_session),valid=not t.errors,has_notifications='notifications' in raw))
            return dict(profiles=profiles,preferences=store.preferences(),last_profile_id=store.last_profile(),revision=store.revision(),unchanged=path.read_bytes()==data)
    except ProfileValidationError as exc:
        text=str(exc)
        error='duplicate' if 'повторяются' in text else 'settings' if 'общие настройки' in text else 'structure' if 'структура профиля' in text else 'read'
        return dict(error=error,unchanged=path.read_bytes()==data)
