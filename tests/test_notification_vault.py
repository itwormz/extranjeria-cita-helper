import pytest
from app.desktop.notification_store import NotificationVault
from app.notifications import NotificationError


class MemoryBackend:
    def __init__(self):self.data={}
    def set_password(self,service,key,value):self.data[(service,key)]=value
    def get_password(self,service,key):return self.data.get((service,key))
    def delete_password(self,service,key):self.data.pop((service,key),None)


def test_vault_roundtrip_and_cancel_cleanup():
    backend=MemoryBackend();vault=NotificationVault(backend)
    ref=vault.put({'token':'SECRET','url':'https://example.test/SECRET'})
    assert ref.startswith('notifications-') and 'SECRET' not in ref
    assert vault.get(ref)['token']=='SECRET'
    vault.delete(ref)
    with pytest.raises(NotificationError):vault.get(ref)


def test_vault_errors_do_not_leak_backend_payload():
    class Broken(MemoryBackend):
        def set_password(self,*args):raise RuntimeError('SECRET in keychain error')
    with pytest.raises(NotificationError) as error:NotificationVault(Broken()).put({'token':'SECRET'})
    assert 'SECRET' not in str(error.value)


def test_profile_stores_only_credential_references(tmp_path,catalog_path,task_mapping):
    from app.desktop.profiles import ProfileStore
    import copy
    store=ProfileStore(tmp_path/'tasks.yml',catalog_path)
    data=copy.deepcopy(task_mapping['tasks'][0]);data['request']['access_mode']='certificate'
    data['applicant']['contact']={'phone':'600000000','email':'demo@example.com'}
    data['notifications']={'telegram':{'enabled':True,'secret_ref':'notifications-demo','events':['booking'],'recipients':[{'id':'123','label':'Demo'}]}}
    profile_id=store.save_profile(None,data,revision=store.revision())
    assert store.raw_profile(profile_id)['notifications']['telegram']['secret_ref']=='notifications-demo'


def test_large_webhook_fits_native_credential_item_limit():
    class BoundedBackend(MemoryBackend):
        def set_password(self,service,key,value):
            if len(value.encode('utf-16-le'))>2560:raise ValueError('native blob limit')
            super().set_password(service,key,value)
    backend=BoundedBackend();vault=NotificationVault(backend)
    data={'body':'x'*6000,'token':'DEMO'}
    reference=vault.put(data)
    assert vault.get(reference)==data
    vault.delete(reference)
    assert not backend.data
