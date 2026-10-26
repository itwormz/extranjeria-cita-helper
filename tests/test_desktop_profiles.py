import os
import copy
import yaml
import pytest


def test_profile_action_timing_defaults_and_invalid_range(tmp_path,catalog_path):
    store=ProfileStore(tmp_path/'tasks.yml',catalog_path)
    store.path.write_text('tasks:\n  - id: test\n    timings:\n      action_delay_seconds: [0.2, 0.6]\n', encoding="utf-8")
    assert store.settings('test').browser_action_delay_seconds == (0.2,0.6)
    store.path.write_text('tasks:\n  - id: test\n', encoding="utf-8")
    assert store.settings('test').browser_action_delay_seconds == (0.3,0.8)
    store.path.write_text('tasks:\n  - id: test\n    timings:\n      action_delay_seconds: [2, 1]\n', encoding="utf-8")
    with pytest.raises(ProfileValidationError): store.settings('test')

from app.desktop.profiles import ProfileStore, ProfileConflictError, ProfileValidationError


def fields(raw):
    task = copy.deepcopy(raw['tasks'][0])
    task['request']['access_mode'] = 'certificate'
    task['applicant']['contact'] = {'phone': '600000000', 'email': 'demo@example.com'}
    return task


def test_action_timings_saved_per_profile(tmp_path,catalog_path,task_mapping):
    store=ProfileStore(tmp_path/'tasks.yml',catalog_path)
    data=fields(task_mapping);data['timings']={'action_delay_seconds':[.4,1.2]}
    first=store.save_profile(None,data,revision=store.revision())
    second=store.save_profile(None,fields(task_mapping),revision=store.revision())
    reopened=ProfileStore(store.path,catalog_path)
    assert reopened.settings(first).browser_action_delay_seconds == (.4,1.2)
    assert reopened.settings(second).browser_action_delay_seconds == (.3,.8)


def test_create_and_preferences_in_missing_file(tmp_path, catalog_path, task_mapping):
    store = ProfileStore(tmp_path / 'tasks.yml', catalog_path)
    assert store.load() == []
    profile_id = store.save_profile(None, fields(task_mapping), revision=store.revision())
    assert store.load()[0].id == profile_id
    if os.name != "nt":
        assert store.path.stat().st_mode & 0o777 == 0o600
    store.save_preferences(dark=True, diagnostics=False, sound=False)
    assert ProfileStore(store.path, catalog_path).preferences() == dict(dark=True, diagnostics=False, sound=False, language='en', automatic_updates=True)


def test_preserves_settings_neighbors_and_detects_conflict(config_file, catalog_path, task_mapping):
    store = ProfileStore(config_file, catalog_path)
    original = yaml.safe_load(config_file.read_text(encoding="utf-8"))
    old_task = copy.deepcopy(original['tasks'][0])
    new_id = store.save_profile(None, fields(task_mapping), revision=store.revision())
    saved = yaml.safe_load(config_file.read_text(encoding="utf-8"))
    assert saved['settings'] == original['settings']
    assert saved['tasks'][0] == old_task
    assert saved['tasks'][1]['id'] == new_id
    revision = store.revision()
    config_file.write_text(config_file.read_text(encoding="utf-8") + '\n# external change\n', encoding="utf-8")
    with pytest.raises(ProfileConflictError):
        store.save_profile(new_id, fields(task_mapping), revision=revision)


@pytest.mark.parametrize('field,value', [('document_number', 'SECRET_INVALID'), ('contact', {'phone': '', 'email': 'invalid'})])
def test_invalid_profile_does_not_write(tmp_path, catalog_path, task_mapping, field, value):
    store = ProfileStore(tmp_path / 'tasks.yml', catalog_path)
    data = fields(task_mapping)
    data['applicant'][field] = value
    with pytest.raises(ProfileValidationError) as error:
        store.save_profile(None, data, revision=store.revision())
    assert 'SECRET_INVALID' not in str(error.value)
    assert not store.path.exists()


def test_malformed_file_is_not_overwritten(tmp_path, catalog_path):
    path = tmp_path / 'tasks.yml'; path.write_text('tasks: [broken', encoding="utf-8")
    store = ProfileStore(path, catalog_path)
    with pytest.raises(ProfileValidationError):
        store.load()
    assert path.read_text(encoding="utf-8") == 'tasks: [broken'


def test_null_settings_and_invalid_desktop_have_safe_errors(tmp_path,catalog_path):
    path=tmp_path/'tasks.yml';path.write_text('settings: null\ntasks: []\n', encoding="utf-8")
    store=ProfileStore(path,catalog_path)
    assert store.preferences()==dict(dark=False,diagnostics=False,sound=True,language='en',automatic_updates=True)
    path.write_text('desktop: true\ntasks: []\n', encoding="utf-8")
    with pytest.raises(ProfileValidationError):store.preferences()


def test_existing_clave_without_contacts_cannot_start(config_file,catalog_path):
    raw=yaml.safe_load(config_file.read_text(encoding="utf-8"))
    raw['tasks'][0]['request']['access_mode']='clave'
    raw['tasks'][0]['applicant'].pop('contact',None)
    config_file.write_text(yaml.safe_dump(raw), encoding="utf-8")
    task=ProfileStore(config_file,catalog_path).load()[0]
    assert any('contact.phone' in e for e in task.errors)
    assert any('contact.email' in e for e in task.errors)


def test_copy_identity_mode_roundtrips_without_manual_identity(tmp_path,catalog_path,task_mapping):
    store=ProfileStore(tmp_path/'tasks.yml',catalog_path)
    data=fields(task_mapping)
    data['applicant'].update(fill_mode='copy',document_number='',full_name='')
    profile_id=store.save_profile(None,data,revision=store.revision())
    task=store.load()[0]
    assert task.id==profile_id and task.applicant_fill_mode=='copy'
    assert not task.errors
    data['request']['access_mode']='without_clave'
    with pytest.raises(ProfileValidationError):
        store.save_profile(profile_id,data,revision=store.revision())


def test_unknown_identity_mode_is_rejected(tmp_path,catalog_path,task_mapping):
    store=ProfileStore(tmp_path/'tasks.yml',catalog_path)
    data=fields(task_mapping);data['applicant']['fill_mode']='invalid'
    with pytest.raises(ProfileValidationError):store.save_profile(None,data,revision=store.revision())


def test_diagnostics_default_off(tmp_path, catalog_path):
    store = ProfileStore(tmp_path / 'tasks.yml', catalog_path)
    assert store.preferences()['diagnostics'] is False
    store.save_preferences(dark=False, diagnostics=True, sound=True)
    assert ProfileStore(store.path, catalog_path).preferences()['diagnostics'] is True


def test_clave_method_defaults_roundtrips_and_rejects_unknown(tmp_path,catalog_path,task_mapping):
    store=ProfileStore(tmp_path/'profiles.yml',catalog_path)
    data=fields(task_mapping);data['request']['access_mode']='clave'
    profile_id=store.save_profile(None,data,revision=store.revision())
    assert store.load()[0].clave_method=='movil'
    data['request']['clave_method']='permanente'
    store.save_profile(profile_id,data,revision=store.revision())
    assert store.load()[0].clave_method=='permanente'
    data['request']['clave_method']='unrecognized'
    with pytest.raises(ProfileValidationError):store.save_profile(profile_id,data,revision=store.revision())


def test_automatic_updates_default_on_and_can_be_disabled(tmp_path,catalog_path):
    store=ProfileStore(tmp_path/'profiles.yml',catalog_path)
    assert store.preferences()['automatic_updates'] is True
    store.save_preferences(dark=False,diagnostics=False,sound=True,language='en',automatic_updates=False)
    assert ProfileStore(store.path,catalog_path).preferences()['automatic_updates'] is False


def test_last_work_profile_survives_preferences_and_store_reopen(tmp_path,catalog_path,task_mapping):
    store=ProfileStore(tmp_path/'tasks.yml',catalog_path)
    store.save_profile(None,fields(task_mapping),revision=store.revision())
    second=store.save_profile(None,fields(task_mapping),revision=store.revision())
    store.save_last_profile(second)
    store.save_preferences(dark=True,diagnostics=False,sound=True)
    assert ProfileStore(store.path,catalog_path).last_profile()==second
    raw=store._read();raw['tasks']=[task for task in raw['tasks'] if task['id']!=second]
    store._write(raw,store.revision())
    assert store.last_profile()==second  # UI resolves missing IDs against the current list.


def test_profile_no_slots_mode_defaults_and_validation(tmp_path,catalog_path,task_mapping):
    store=ProfileStore(tmp_path/'tasks.yml',catalog_path)
    raw=fields(task_mapping);first=store.save_profile(None,raw,revision=store.revision())
    assert store.load()[0].no_slots_mode=='reload'
    raw['request']['no_slots_mode']='salir';store.save_profile(first,raw,revision=store.revision())
    assert ProfileStore(store.path,catalog_path).load()[0].no_slots_mode=='salir'
    raw['request']['no_slots_mode']='unsupported'
    with pytest.raises(ProfileValidationError):store.save_profile(first,raw,revision=store.revision())
