"""Procedure-specific applicant validation; no real personal data."""
import copy
import pytest
from app.config import Settings, _task, _load_catalog

REGRESO = 'AUTORIZACIÓN DE REGRESO'

@pytest.mark.parametrize('kind,number', [('NIE','X0000000T'),('DNI','00000000T'),('PASSPORT','PA1234567')])
def test_regreso_documents_do_not_require_nationality(task_mapping,catalog_path,kind,number):
    raw=copy.deepcopy(task_mapping['tasks'][0]);raw['request']['procedure']=REGRESO
    raw['applicant'].update(document_type=kind,document_number=number,nationality='')
    task=_task(raw,Settings(),_load_catalog(catalog_path))
    assert task.valid,task.errors
    assert task.document_type==kind

@pytest.mark.parametrize('kind,number',[('DNI','00000000A'),('DNI','1234T'),('PASSPORT',''),('PASSPORT','PA<script>'),('PASSPORT','A'*21)])
def test_regreso_rejects_invalid_document(task_mapping,catalog_path,kind,number):
    raw=copy.deepcopy(task_mapping['tasks'][0]);raw['request']['procedure']=REGRESO
    raw['applicant'].update(document_type=kind,document_number=number,nationality='')
    task=_task(raw,Settings(),_load_catalog(catalog_path))
    assert any('document_number' in error for error in task.errors)

@pytest.mark.parametrize('kind,number',[('DNI','00000000T'),('PASSPORT','PA1234567')])
def test_huellas_still_requires_nie(task_mapping,catalog_path,kind,number):
    raw=copy.deepcopy(task_mapping['tasks'][0]);raw['applicant'].update(document_type=kind,document_number=number)
    task=_task(raw,Settings(),_load_catalog(catalog_path))
    assert any('document_type' in error for error in task.errors)


def test_regreso_copy_uses_authorized_identity_without_nationality(task_mapping,catalog_path):
    raw=copy.deepcopy(task_mapping['tasks'][0]);raw['request'].update(procedure=REGRESO,access_mode='clave')
    raw['applicant'].update(fill_mode='copy',document_type='NIE',document_number='',full_name='',nationality='')
    assert _task(raw,Settings(browser_session='persistent'),_load_catalog(catalog_path)).valid
