"""Canonical profile drafts checked by the existing Python config validator."""
from dataclasses import asdict
from app.config import _task, Settings, TIE_PROCEDURE, REGRESO_PROCEDURE


def validate(draft, catalog):
    raw = dict(id=draft['id'], label=draft['label'], enabled=draft['enabled'],
        request=dict(province=draft['province'], office=draft['office'],
                     procedure={'huellas':TIE_PROCEDURE,'regreso':REGRESO_PROCEDURE}.get(draft['procedure'],draft['procedure']),
                     access_mode=draft['access_mode'],clave_method=draft['clave_method'],
                     no_slots_mode=draft['no_slots_mode'],applicant_no_slots_mode=draft['applicant_no_slots_mode'],
                     subprocedure=draft['subprocedure']),
        applicant=dict(document_type=draft['document_type'],document_number=draft['document_number'],
                       full_name=draft['full_name'],nationality=draft['nationality'],fill_mode=draft['fill_mode'],
                       contact=dict(phone=draft['phone'],email=draft['email'])),
        schedule=dict(timezone=draft['timezone']))
    task=_task(raw,Settings(browser_session=draft['browser_session']), {key:set(value) for key,value in catalog.items()})
    normalized=dict(draft)
    normalized.update(document_type=task.document_type,document_number=task.document_number,
        full_name=task.full_name,phone=task.contact_phone,email=task.contact_email,
        access_mode=task.access_mode,clave_method=task.clave_method,fill_mode=task.applicant_fill_mode,
        no_slots_mode=task.no_slots_mode,applicant_no_slots_mode=task.applicant_no_slots_mode)
    return dict(normalized=normalized,errors=list(task.errors))
