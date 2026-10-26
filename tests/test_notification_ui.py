import os
os.environ.setdefault('QT_QPA_PLATFORM','offscreen')
import pytest
pytest.importorskip('PySide6')
from PySide6.QtWidgets import QLineEdit
from app.desktop.notification_widgets import NotificationPanel, RecipientDialog, WebhookDialog
from app.desktop.profile_widgets import ProfileField
from app.desktop.window import DesktopWindow
from PySide6.QtCore import QObject, Signal

class Controller(QObject):
    state_changed=Signal(dict)
    failed=Signal(str)
    finished=Signal(int)
    active=False
    def send(self,message):pass
    def shutdown(self,**kwargs):self.active=False
from app.desktop.profiles import ProfileStore


@pytest.fixture
def owner(qt_app,tmp_path,catalog_path):
    window=DesktopWindow(ProfileStore(tmp_path/'tasks.yml',catalog_path),Controller())
    yield window
    window.close();window.deleteLater();qt_app.processEvents()


def test_notifications_channels_hide_and_fields_reuse_guidelines(owner,qt_app):
    panel=owner.notifications_panel
    panel.load({})
    owner.create_profile();owner.profile_tabs.setCurrentWidget(panel.scroll)
    qt_app.processEvents()
    assert panel.telegram_body.isHidden() and panel.webhook_body.isHidden()
    panel.telegram_toggle.setChecked(True)
    assert not panel.telegram_body.isHidden()
    assert isinstance(panel.token_box,ProfileField)
    assert panel.token_box.height()==48
    assert panel.token.echoMode()==QLineEdit.EchoMode.Password
    panel.eye.click();assert panel.token.echoMode()==QLineEdit.EchoMode.Normal
    panel.telegram_toggle.setChecked(False)
    assert panel.telegram_body.isHidden()


def test_advanced_rows_have_one_plus_fixed_delete_and_secret_defaults(owner):
    dialog=WebhookDialog(owner,{})
    assert len(dialog.headers.rows)==1
    first=dialog.headers.rows[0]
    assert first.secret.isChecked()
    first.plus.click()
    assert len(dialog.headers.rows)==2
    assert sum(not row.plus.isHidden() for row in dialog.headers.rows)==1
    assert first.remove.x()==dialog.headers.rows[-1].remove.x()
    dialog.headers.rows[-1].remove.click()
    assert len(dialog.headers.rows)==1
    assert first.value.echoMode()==QLineEdit.EchoMode.Password
    first.secret.click();assert first.value.echoMode()==QLineEdit.EchoMode.Normal
    assert dialog.schema.isHidden()
    dialog.reject()


def test_pairing_requires_found_recipient_and_explicit_confirmation(owner):
    dialog=RecipientDialog(owner,'123456:DEMO_token')
    assert not dialog.add_button.isEnabled()
    dialog.found({'id':'42','label':'Demo'})
    assert not dialog.add_button.isEnabled()
    assert dialog.result()==0
    dialog.candidate_button.click();dialog.add_button.click()
    assert dialog.recipient=={'id':'42','label':'Demo'}


def test_saving_profile_keeps_tokens_out_of_yaml_and_cancel_does_not_write(owner):
    from app.desktop.notification_store import NotificationVault
    class Backend:
        def __init__(self):self.data={}
        def set_password(self,service,key,value):self.data[key]=value
        def get_password(self,service,key):return self.data.get(key)
        def delete_password(self,service,key):self.data.pop(key,None)
    backend=Backend();panel=owner.notifications_panel;panel.vault=NotificationVault(backend)
    owner.create_profile()
    for key,value in {'label':'Demo','full_name':'NOMBRE APELLIDO','document_number':'X0000000T','phone':'600000000','email':'demo@example.com'}.items():owner.fields[key].setText(value)
    owner.profile_tabs.setCurrentIndex(2)
    panel.token.setText('123456:SECRET_token');panel.telegram_toggle.setChecked(True);panel.recipients=[{'id':'42','label':'Demo'}]
    owner.fields['nationality'].setCurrentText('UCRANIA')
    owner.save_profile()
    assert not owner.profile_editor.isVisible(),owner.form_error.text()
    assert 'SECRET_token' not in owner.store.path.read_text()
    assert backend.data
    count=len(backend.data);owner.open_editor(owner.profile_combo.currentData());panel.token.setText('123456:OTHER_secret');owner.profile_editor.reject()
    assert len(backend.data)==count
    assert 'OTHER_secret' not in str(backend.data)


def test_recoverable_protocol_error_does_not_notify_as_terminal(owner):
    observed=[];owner.notification_delivery.observe=lambda *args:observed.append(args)
    owner.controller.failed.emit('Не удалось прочитать сообщение бота — ждём следующее обновление.')
    assert not observed
    owner.on_finished(1)
    assert observed[-1][1]['failed']


def test_dynamic_webhook_selector_uses_the_same_themed_popup(owner):
    from app.desktop.window import ThemedItemDelegate
    dialog=WebhookDialog(owner,{})
    assert isinstance(dialog.body_mode.view().itemDelegate(),ThemedItemDelegate)
    assert 'combobox-popup: 0' in dialog.body_mode.styleSheet()
    assert not dialog.body_field.icon.isVisible()
    assert not dialog.headers.rows[0].name_box.icon.isVisible()
    dialog.reject()


def test_telegram_settings_fit_default_editor_without_scroll(owner,qt_app):
    owner.create_profile();panel=owner.notifications_panel
    panel.telegram_toggle.setChecked(True);panel.webhook_toggle.setChecked(False)
    panel.recipients=[{'id':'123456789','label':'Demo'}];panel.render_recipients()
    owner.profile_tabs.setCurrentWidget(panel.scroll);owner.profile_editor.resize(650,580)
    qt_app.processEvents()
    assert panel.scroll.verticalScrollBar().maximum()==0


def test_json_disclosure_has_no_nested_scroll_widget(owner,qt_app):
    from PySide6.QtWidgets import QAbstractScrollArea,QLabel
    dialog=WebhookDialog(owner,{'url':'https://example.test'})
    assert isinstance(dialog.schema,QLabel)
    assert isinstance(dialog.schema_button,QLabel)
    dialog.schema_button.linkActivated.emit('#')
    dialog.show();qt_app.processEvents()
    assert dialog.schema.isVisible()
    assert not isinstance(dialog.schema,QAbstractScrollArea)
    dialog.close()


def test_event_summary_preserves_custom_selection_and_supports_all(owner):
    from PySide6.QtCore import Qt
    events=owner.notifications_panel.telegram_events
    events.load(['office','booking'])
    assert events.details.isHidden()
    assert events.all_events.checkState()==Qt.CheckState.PartiallyChecked
    assert events.value()==['office','booking']
    events.disclosure.setChecked(True)
    assert not events.details.isHidden()
    events.all_events.click()
    assert events.value()==['auth','office','booking','error']
    events.all_events.click()
    assert events.value()==[]


def test_recipient_test_targets_only_clicked_chat(owner,monkeypatch):
    from PySide6.QtWidgets import QPushButton
    from app.desktop import notification_widgets as module
    sent=[]
    class Client:
        def __init__(self,token):pass
        def send(self,chat,message):sent.append(chat)
    monkeypatch.setattr(module,'Telegram',Client)
    panel=owner.notifications_panel;panel.token.setText('123456:DEMO')
    panel.recipients=[{'id':'1','label':'First'},{'id':'2','label':'Second'}];panel.render_recipients()
    panel.jobs.submit=lambda work,done:done(True,work())
    tests=[b for b in panel.findChildren(QPushButton) if b.property('recipientId')=='2']
    assert len(tests)==1
    tests[0].click()
    assert sent==['2']


def test_manual_mode_invalidates_found_choice_and_never_overlaps(owner,qt_app):
    dialog=RecipientDialog(owner,'123456:DEMO_token');dialog.set_mode('manual')
    dialog.show();qt_app.processEvents()
    assert dialog.auto.isHidden() and not dialog.manual.isHidden()
    dialog.found({'id':'42','label':'Demo','type':'private','username':'demo_user'})
    assert not dialog.add_button.isEnabled()
    dialog.candidate_button.click();assert dialog.add_button.isEnabled()
    dialog.chat_id.setText('99');assert not dialog.add_button.isEnabled()
    assert dialog.candidate_button.isHidden()
    field=dialog.chat_id.parentWidget()
    assert field.mapTo(dialog,field.rect().bottomLeft()).y()<dialog.manual_hint.mapTo(dialog,dialog.manual_hint.rect().topLeft()).y()
    dialog.reject()


def test_test_result_changes_button_without_new_status_row(owner,qt_app,monkeypatch):
    from PySide6.QtWidgets import QPushButton
    from app.desktop import notification_widgets as module
    class Client:
        def __init__(self,token):pass
        def send(self,*args):return 15
    monkeypatch.setattr(module,'Telegram',Client)
    panel=owner.notifications_panel;panel.token.setText('123456:DEMO')
    panel.recipients=[{'id':'42','label':'Demo'}];panel.render_recipients()
    control=next(b for b in panel.findChildren(QPushButton) if b.property('recipientId')=='42')
    original=control.size()
    panel.jobs.submit=lambda work,done:done(True,work())
    control.click()
    assert control.text()==owner.t('Принято')
    assert control.size()==original and panel.telegram_status.isHidden()
    assert panel.test_webhook.height()==36
    assert panel.url_box.height()==48


def test_dialog_does_not_release_another_dialogs_pairing_lease(owner):
    token='123456:DEMO_lease';pending=[]
    owner.notifications_panel.jobs.submit=lambda work,done:pending.append(done)
    first=RecipientDialog(owner,token);first.start_pairing(False)
    second=RecipientDialog(owner,token)
    assert token in RecipientDialog._polling_tokens
    second.start_pairing(False);assert len(pending)==1
    first.reject();assert token in RecipientDialog._polling_tokens
    pending[0](False,'HTTP 409');assert token not in RecipientDialog._polling_tokens
    second.reject()


def test_found_chat_button_contents_fit_its_actual_height(owner,qt_app):
    dialog=RecipientDialog(owner,'123456:DEMO_token')
    dialog.found({'id':'42','label':'Demo','type':'private','username':'demo_user'})
    dialog.show();qt_app.processEvents()
    choice=dialog.candidate_button
    assert choice.height()>=70
    assert choice.details.geometry().bottom()<choice.height()
    dialog.reject()


def test_profile_editor_defers_keychain_and_preserves_notification_refs(owner,task_mapping):
    from tests.test_desktop_profiles import fields
    from unittest.mock import Mock
    data=fields(task_mapping)
    data['notifications']={'telegram':{'enabled':True,'secret_ref':'keep-tg','events':['booking'],'recipients':[{'id':'123','label':'Demo'}]},'webhook':{'enabled':False,'secret_ref':'keep-hook'}}
    profile=owner.store.save_profile(None,data,revision=owner.store.revision())
    panel=owner.notifications_panel
    panel.vault=Mock();panel.vault.get.return_value={'token':'demo-token'}
    owner.refresh_profiles(selected=profile);owner.open_editor(profile)
    assert panel.vault.get.call_count==0
    owner.fields['label'].setText('Changed without notifications')
    owner.save_profile()
    assert panel.vault.get.call_count==0 and panel.vault.put.call_count==0 and panel.vault.delete.call_count==0
    saved=owner.store.raw_profile(profile)['notifications']
    assert saved['telegram']['secret_ref']=='keep-tg' and saved['webhook']['secret_ref']=='keep-hook'
    owner.open_editor(profile);owner.profile_tabs.setCurrentIndex(2)
    assert panel.vault.get.call_count==2
    owner.profile_tabs.setCurrentIndex(0);owner.profile_tabs.setCurrentIndex(2)
    assert panel.vault.get.call_count==2


def test_profile_editor_reloads_secrets_when_notifications_tab_is_already_open(owner,task_mapping):
    from tests.test_desktop_profiles import fields
    from unittest.mock import Mock
    panel=owner.notifications_panel
    panel.vault=Mock()
    panel.vault.get.side_effect=lambda ref: {'token':ref} if ref else {}
    profiles=[]
    for ref in ('first-token','second-token'):
        data=fields(task_mapping)
        data['notifications']={'telegram':{'enabled':False,'secret_ref':ref}}
        profiles.append(owner.store.save_profile(None,data,revision=owner.store.revision()))
    owner.open_editor(profiles[0]);owner.profile_tabs.setCurrentIndex(2)
    assert panel.token.text()=='first-token'
    owner.open_editor(profiles[1])
    assert panel.token.text()=='second-token'
    assert panel.vault.get.call_count==4
