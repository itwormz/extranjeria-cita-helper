import json
import threading
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from urllib.request import Request
import pytest
from app import notifications as n


def test_real_http_delivery_keeps_event_id_and_does_not_follow_redirect():
    seen=[]
    class Handler(BaseHTTPRequestHandler):
        def do_POST(self):
            seen.append((self.path,self.rfile.read(int(self.headers.get('Content-Length',0)))))
            self.send_response(302 if self.path=='/redirect' else 200)
            self.send_header('Location','/other');self.end_headers();self.wfile.write(b'{"ok":true}')
        def log_message(self,*args):pass
    server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
    thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    try:
        url=f'http://127.0.0.1:{server.server_port}'
        status,body,_=n.send_request(Request(url+'/ok',data=b'{"event_id":"test-id"}'),attempts=1)
        assert status==200 and json.loads(body)['ok']
        with pytest.raises(n.NotificationError,match='HTTP 302'):
            n.send_request(Request(url+'/redirect',data=b'private'),attempts=1)
        assert [row[0] for row in seen]==['/ok','/redirect']
    finally:server.shutdown();server.server_close();thread.join()


def test_queued_delivery_expires_when_browser_moves_on(qt_app):
    from app.desktop.notification_delivery import NotificationDelivery
    class Store:
        def raw_profile(self,profile):
            return {'label':'Demo','request':{'province':'Barcelona'},'notifications':{'webhook':{'enabled':True,'events':['booking'],'secret_ref':'ref'}}}
    delivery=NotificationDelivery(Store());queued=[]
    delivery.jobs.submit=lambda work,done:queued.append(work)
    delivery.observe('one',{'screen':'Выбор ситы','mode':'manual','booking_handoff':True},'en')
    assert len(queued)==1
    delivery.observe('one',{'screen':'Форма заявителя','mode':'working'},'en')
    with pytest.raises(n.NotificationError,match='устарело'):queued[0]()
    delivery.stop()


def test_pairing_code_is_not_accepted_as_part_of_other_text():
    pair=n.Pairing('demo_bot')
    assert pair.match([{'update_id':1,'message':{'text':'hello /start '+pair.code,'chat':{'id':42,'type':'private'}}}],'private') is None


def test_finished_booking_does_not_keep_delivery_alive():
    assert n.state_event({'booking_handoff':True,'finished':True,'mode':'finished'}) is None


def test_terminal_failure_has_priority_over_pause_and_lost_connection():
    assert n.EventRouter().observe({'failed':True,'paused':True,'connection_lost':True})=='error'


def test_unreachable_telegram_chat_does_not_prevent_other_recipients(qt_app,monkeypatch):
    from app.desktop import notification_delivery as module
    class Store:
        def raw_profile(self,profile):
            return {'label':'Demo','notifications':{'telegram':{'enabled':True,'events':['booking'],'secret_ref':'ref','recipients':[{'id':'1'},{'id':'2'}]}}}
    delivered=[]
    class Client:
        def __init__(self,token):pass
        def send(self,chat,message,current):
            if chat=='1':raise n.NotificationError('HTTP 403')
            delivered.append(chat)
    monkeypatch.setattr(module,'Telegram',Client)
    delivery=module.NotificationDelivery(Store());delivery.vault.get=lambda reference:{'token':'test'}
    delivery.prepare('one')
    delivery.jobs.submit=lambda work,done:done(True,work())
    delivery.observe('one',{'booking_handoff':True,'mode':'manual'},'en')
    assert delivered==['2']
    delivery.stop()


def test_telegram_waits_for_details_once_but_webhook_stays_generic(qt_app,monkeypatch):
    from app.desktop import notification_delivery as module
    class Store:
        def raw_profile(self,profile):
            return {'label':'Demo','notifications':{
                'telegram':{'enabled':True,'events':['booking'],'secret_ref':'tg','recipients':[{'id':'1'}]},
                'webhook':{'enabled':True,'events':['booking'],'secret_ref':'web'}}}
    telegram=[];webhook=[]
    class Client:
        def __init__(self,token):pass
        def send(self,chat,message,current):telegram.append(message)
    monkeypatch.setattr(module,'Telegram',Client)
    def send(request,**kwargs):webhook.append(json.loads(request.data));return 200,b'',1
    monkeypatch.setattr(module,'send_request',send)
    delivery=module.NotificationDelivery(Store())
    delivery.vault.get=lambda ref:{'token':'test'} if ref=='tg' else {'url':'https://example.test'}
    delivery.prepare('one')
    delivery.jobs.submit=lambda work,done:done(True,work())
    state={'booking_handoff':True,'mode':'manual','run_id':'one','booking_details_pending':True}
    delivery.observe('one',state,'en')
    assert len(webhook)==1 and not telegram
    ready=dict(state,booking_details_pending=False,booking_details={'office':'CNP TEST','offers':[{'date':'2026-10-28','time':'12:30'}]})
    delivery.observe('one',ready,'en');delivery.observe('one',ready,'en')
    assert len(telegram)==1 and 'CNP TEST' in telegram[0] and '12:30' in telegram[0]
    assert 'CNP TEST' not in str(webhook) and len(webhook)==1
    delivery.stop()


def test_telegram_empty_details_falls_back_once_after_read(qt_app,monkeypatch):
    from app.desktop import notification_delivery as module
    class Store:
        def raw_profile(self,profile):return {'label':'Demo','notifications':{'telegram':{'enabled':True,'events':['booking'],'secret_ref':'tg','recipients':[{'id':'1'}]}}}
    sent=[]
    class Client:
        def __init__(self,token):pass
        def send(self,chat,message,current):sent.append(message)
    monkeypatch.setattr(module,'Telegram',Client)
    delivery=module.NotificationDelivery(Store());delivery.vault.get=lambda ref:{'token':'test'}
    delivery.prepare('one')
    delivery.jobs.submit=lambda work,done:done(True,work())
    state={'booking_handoff':True,'mode':'manual','run_id':'one','booking_details_pending':True}
    delivery.observe('one',state,'ru');assert not sent
    state.update(booking_details_pending=False,booking_details={})
    delivery.observe('one',state,'ru');delivery.observe('one',state,'ru')
    assert len(sent)==1 and 'Открыт выбор ситы' in sent[0] and 'CAPTCHA' in sent[0]
    delivery.stop()


def test_booking_delivery_uses_preloaded_secret_without_reading_vault(qt_app, monkeypatch):
    from unittest.mock import Mock
    from app.desktop import notification_delivery as module

    class Store:
        def raw_profile(self, profile):
            return {'label': 'Demo', 'notifications': {'telegram': {
                'enabled': True, 'events': ['booking'], 'secret_ref': 'tg',
                'recipients': [{'id': '1'}]}}}
    sent = []
    class Client:
        def __init__(self, token):
            assert token == 'synthetic-token'
        def send(self, chat, message, current):
            sent.append(chat)
    monkeypatch.setattr(module, 'Telegram', Client)
    delivery = module.NotificationDelivery(Store())
    delivery.vault.get = Mock(return_value={'token': 'synthetic-token'})
    delivery.prepare('one')
    delivery.vault.get.assert_called_once_with('tg')
    delivery.vault.get.side_effect = AssertionError('Vault must not be accessed during booking')
    delivery.jobs.submit = lambda work, done: done(True, work())

    delivery.observe('one', {'booking_handoff': True, 'mode': 'manual'}, 'en')

    assert sent == ['1']
    assert delivery.vault.get.call_count == 1
    delivery.stop()
    assert not delivery.secrets


def test_unprepared_booking_never_prompts_for_vault_access(qt_app):
    from unittest.mock import Mock
    from app.desktop.notification_delivery import NotificationDelivery

    class Store:
        def raw_profile(self, profile):
            return {'notifications': {'telegram': {
                'enabled': True, 'events': ['booking'], 'secret_ref': 'tg'}}}
    delivery = NotificationDelivery(Store())
    delivery.vault.get = Mock(side_effect=AssertionError('Unexpected vault access'))
    queued = []
    delivery.jobs.submit = lambda work, done: queued.append(work)
    delivery.observe('one', {'booking_handoff': True, 'mode': 'manual'}, 'en')
    with pytest.raises(n.NotificationError, match='не подготовлены'):
        queued[0]()
    delivery.vault.get.assert_not_called()
    delivery.stop()


def test_secret_preparation_reports_failure_and_can_be_retried(qt_app):
    from unittest.mock import Mock
    from app.desktop.notification_delivery import NotificationDelivery

    class Store:
        def raw_profile(self, profile):
            return {'notifications': {
                'telegram': {'enabled': True, 'events': ['booking'], 'secret_ref': 'tg'},
                'webhook': {'enabled': True, 'events': ['booking'], 'secret_ref': 'web'}}}

    delivery = NotificationDelivery(Store())
    errors = []
    delivery.result.connect(lambda channel, message: errors.append((channel, message)))
    delivery.vault.get = Mock(side_effect=[n.NotificationError('Vault denied'), {'url': 'https://example.test'}])
    assert delivery.prepare('one') is False
    assert errors == [('telegram', 'Vault denied')]
    delivery.vault.get = Mock(return_value={'token': 'synthetic'})
    assert delivery.prepare('one') is True
    delivery.vault.get.assert_called_once_with('tg')
    delivery.stop()


def test_disabled_notifications_do_not_require_vault_access(qt_app):
    from unittest.mock import Mock
    from app.desktop.notification_delivery import NotificationDelivery
    class Store:
        def raw_profile(self, profile):
            return {'notifications': {}}
    delivery = NotificationDelivery(Store())
    delivery.vault.get = Mock(side_effect=AssertionError('Disabled channels must not read vault'))
    assert delivery.prepare('one') is True
    delivery.vault.get.assert_not_called()
    delivery.stop()


@pytest.mark.parametrize('error', [StopIteration(), ValueError('Unreadable profile')])
def test_profile_read_failure_is_reported_before_start_is_rejected(qt_app, error):
    from app.desktop.notification_delivery import NotificationDelivery
    class Store:
        def raw_profile(self, profile):
            raise error
    delivery = NotificationDelivery(Store())
    errors = []
    delivery.result.connect(lambda channel, message: errors.append((channel, message)))
    assert delivery.prepare('missing') is False
    assert errors == [('profile', 'Не удалось прочитать выбранный профиль. Выберите профиль заново и повторите запуск.')]
    delivery.stop()


def test_office_alert_waits_in_queue_across_polling_and_contains_current_offices(qt_app,monkeypatch):
    from app.assist_control import AssistControl
    from app.desktop import notification_delivery as module
    class Store:
        def raw_profile(self,profile):
            return {'label':'Demo','notifications':{'telegram':{
                'enabled':True,'events':['office'],'secret_ref':'tg','recipients':[{'id':'1'}]}}}
    sent=[]
    class Client:
        def __init__(self,token):pass
        def send(self,chat,message,current):
            assert current()
            sent.append(message)
    monkeypatch.setattr(module,'Telegram',Client)
    delivery=module.NotificationDelivery(Store());delivery.vault.get=lambda ref:{'token':'synthetic'}
    delivery.prepare('one')
    queued=[]
    delivery.jobs.submit=lambda work,done:queued.append(work)
    control=AssistControl();control.set_session('run')
    def publish(mode):
        control.publish(screen='Выбор офиса', evidence='',next_action='Выбрать офис',mode=mode)
        delivery.observe('one',control.snapshot(),'en')
    control.observe_visit(document='office-one',stage='office',page='https://example.test',title='Выбор офиса')
    publish('working')
    control.update_offices([{'value':'private-one','text':'CNP TEST NORTH'},
                            {'value':'private-two','text':'CNP TEST SOUTH'}])
    publish('manual')
    for _ in range(3):
        control.publish(screen='Проверяем страницу',evidence='',next_action='Чтение страницы',mode='waiting')
        delivery.observe('one',control.snapshot(),'en')
        publish('working');publish('manual')
    assert len(queued)==1
    queued[0]()
    assert len(sent)==1 and 'CNP TEST NORTH' in sent[0] and 'CNP TEST SOUTH' in sent[0]
    assert 'private-one' not in sent[0] and 'private-two' not in sent[0]
    # A new actual office document is a new visit and expires the old queued alert.
    control.observe_visit(document='office-two',stage='office',page='https://example.test',title='Выбор офиса')
    control.update_offices([{'value':'private-new','text':'CNP TEST WEST'}])
    publish('manual')
    assert len(queued)==2
    with pytest.raises(n.NotificationError,match='устарело'):queued[0]()
    queued[1]()
    assert 'CNP TEST WEST' in sent[1] and 'CNP TEST NORTH' not in sent[1]
    delivery.stop()


@pytest.mark.parametrize('interruption', ['paused', 'connection_lost'])
def test_queued_office_alert_survives_interruption_but_not_a_new_document(qt_app, monkeypatch, interruption):
    from app.desktop import notification_delivery as module

    class Store:
        def raw_profile(self, profile):
            return {'notifications': {'telegram': {
                'enabled': True, 'events': ['office'], 'secret_ref': 'tg',
                'recipients': [{'id': '1'}]}}}

    sent = []
    class Client:
        def __init__(self, token):
            pass
        def send(self, chat, message, current):
            assert current()
            sent.append(message)

    monkeypatch.setattr(module, 'Telegram', Client)
    delivery = module.NotificationDelivery(Store())
    delivery.vault.get = lambda ref: {'token': 'synthetic'}
    delivery.prepare('one')
    queued = []
    delivery.jobs.submit = lambda work, done: queued.append(work)
    state = {'screen': 'Выбор офиса', 'mode': 'manual', 'run_id': 'run',
             'document': 'office-one', 'stage': 'office', 'available_offices': ['CNP TEST']}
    try:
        delivery.observe('one', state, 'en')
        delivery.observe('one', dict(state, **{interruption: True}), 'en')
        queued[0]()
        assert len(sent) == 1
        delivery.observe('one', state, 'en')
        assert len(queued) == 1
        delivery.observe('one', dict(state, document='office-two'), 'en')
        with pytest.raises(n.NotificationError, match='устарело'):
            queued[0]()
        assert len(queued) == 2
        queued[1]()
        assert len(sent) == 2
    finally:
        delivery.stop()
