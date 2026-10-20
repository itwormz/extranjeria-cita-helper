import json
import pytest
from app import notifications as n


def test_disabled_channels_and_private_event_payload():
    assert not n.defaults()['telegram']['enabled']
    event=n.make_event('booking', 'Demo', 'Barcelona', 'en')
    assert event['event']=='booking'
    assert 'Demo' in event['message']
    assert 'NIE' not in event
    assert n.state_event({'screen':'Выбор ситы','mode':'manual','booking_handoff':True})=='booking'
    assert n.state_event({'screen':'Выбор офиса','mode':'working'}) is None
    assert n.state_event({'screen':'Авторизация сертификатом','mode':'manual'})=='auth'
    assert n.state_event({'screen':'Неизвестный экран','mode':'manual'}) is None


def test_webhook_get_encodes_and_json_template_escapes():
    event=n.make_event('booking', 'A"B', 'Barcelona', 'ru')
    settings=dict(url='https://example.test/hook',method='GET',query=[dict(name='message',value='{{message}}')],headers=[])
    request=n.webhook_request(settings,event)
    assert request.data is None and '%22' in request.full_url
    settings.update(method='POST',body_mode='custom',body='{"text":"{{message}}"}')
    assert json.loads(n.webhook_request(settings,event).data)['text']==event['message']


@pytest.mark.parametrize('patch',[
 {'url':'http://example.test'}, {'url':'https://u:pass@example.test'},
 {'headers':[{'name':'Host','value':'elsewhere'}]},
 {'headers':[{'name':'X-Token','value':'a\r\nb'}]},
 {'headers':[{'name':'x-a','value':'1'},{'name':'X-A','value':'2'}]},
 {'body_mode':'custom','body':'{"text":"{{unknown}}"}'},
])
def test_invalid_webhook_rejected_without_echoing_secrets(patch):
    settings=dict(url='https://example.test',method='POST',headers=[],query=[],body_mode='standard')
    settings.update(patch)
    with pytest.raises(n.NotificationError):n.validate_webhook(settings)


def test_pairing_matches_exact_code_and_chat_type_only():
    pair=n.Pairing('demo_bot',now=10)
    assert pair.code in pair.link('private')
    rows=[{'update_id':1,'message':{'text':'/start wrong','chat':{'id':1,'type':'private'}}},
          {'update_id':2,'message':{'text':'/start '+pair.code,'chat':{'id':2,'type':'private','first_name':'Demo'}}}]
    assert pair.match(rows,'private',now=20)=={'id':'2','label':'Demo','type':'private','username':''}
    assert pair.match(rows,'group',now=20) is None
    assert pair.match(rows,'private',now=131) is None


def test_router_deduplicates_but_allows_new_visit():
    router=n.EventRouter()
    state=dict(screen='Выбор офиса',mode='manual',run_id='one')
    assert router.observe(state)=='office'
    assert router.observe(state) is None
    router.observe(dict(screen='Контактные данные',mode='working',run_id='one'))
    assert router.observe(state)=='office'
    assert router.observe(dict(state,connection_lost=True)) is None


def test_request_failures_never_leak_endpoint_or_header(monkeypatch):
    from urllib.error import URLError
    def fail(*args,**kwargs):raise URLError('https://secret.example/TOKEN')
    monkeypatch.setattr(n,'urlopen',fail)
    with pytest.raises(n.NotificationError) as error:
        n.request_json('https://secret.example/TOKEN',{},attempts=1)
    assert 'TOKEN' not in str(error.value)


def test_telegram_existing_webhook_prevents_pairing(monkeypatch):
    client=n.Telegram('123456:ABCdef')
    monkeypatch.setattr(client,'call',lambda method,data=None:{'url':'https://existing.test'} if method=='getWebhookInfo' else {'username':'demo_bot'})
    with pytest.raises(n.NotificationError):client.begin_pairing()
