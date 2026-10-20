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
    delivery.jobs.submit=lambda work,done:done(True,work())
    delivery.observe('one',{'booking_handoff':True,'mode':'manual'},'en')
    assert delivered==['2']
    delivery.stop()
