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
    assert pair.match(rows,'private',now=309) is not None
    assert pair.match(rows,'private',now=310) is None


def test_router_deduplicates_but_allows_new_visit():
    router=n.EventRouter()
    state=dict(screen='Выбор офиса',mode='manual',run_id='one')
    assert router.observe(state)=='office'
    assert router.observe(state) is None
    router.observe(dict(screen='Контактные данные',mode='working',run_id='one'))
    assert router.observe(state)=='office'
    assert router.observe(dict(state,connection_lost=True)) is None


def test_office_polling_does_not_repeat_alert_for_the_same_document():
    router=n.EventRouter()
    state=dict(screen='Выбор офиса', mode='manual', run_id='one', document='office-one',stage='office')
    assert router.observe(dict(state, mode='working')) is None
    assert router.observe(state)=='office'
    for _ in range(3):
        assert router.observe(dict(state,screen='Проверяем страницу',mode='waiting')) is None
        assert router.observe(dict(state, mode='working')) is None
        assert router.observe(state) is None
    assert router.observe(dict(state, document='office-two'))=='office'


@pytest.mark.parametrize('screen,event', [('Cl@ve Móvil','auth'),('Выбор ситы','booking')])
def test_refresh_during_other_manual_steps_does_not_rearm_the_alert(screen,event):
    router=n.EventRouter()
    state=dict(screen=screen,mode='manual',run_id='one',document='first')
    assert router.observe(state)==event
    assert router.observe(dict(state,document='refreshed')) is None


@pytest.mark.parametrize('language', ['ru','en','es'])
def test_telegram_office_alert_lists_public_names_without_changing_webhook(language):
    event=n.make_event('office','Demo','Barcelona',language)
    message=event['message']
    text=n.telegram_message(event,language,offices=['CNP TEST NORTH, CALLE TEST 1',
                                                'CNP TEST SOUTH, CALLE TEST 2'])
    assert 'CNP TEST NORTH, CALLE TEST 1' in text
    assert 'CNP TEST SOUTH, CALLE TEST 2' in text
    assert event['message']==message
    assert n.telegram_message(event,language,offices=[])==message


def test_telegram_office_list_is_bounded_and_ignores_non_text_values():
    event=n.make_event('office','Demo','Barcelona','en')
    names=['CNP TEST '+str(i)+' '+('A'*400) for i in range(20)]
    text=n.telegram_message(event,'en',offices=[None,{'value':'private-id'},*names,names[0]])
    assert len(text)<4096
    assert 'private-id' not in text and 'None' not in text
    assert 'CNP TEST 0 ' in text and 'More offices: 10' in text
    assert text.count('CNP TEST 0 ')==1


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


@pytest.mark.parametrize('language',['ru','en','es'])
def test_telegram_booking_text_contains_public_offers_only(language):
    details={'office':'CNP TEST, CALLE TEST, 1','offers':[{'date':'2026-10-28','time':'12:30'}],
             'document_number':'PRIVATE-NIE','confirmation':'PRIVATE-CODE'}
    event=n.make_event('booking','Demo','Barcelona',language)
    text=n.telegram_message(event,language,details)
    assert 'CNP TEST, CALLE TEST, 1' in text and '28.10.2026 — 12:30' in text
    assert 'PRIVATE-' not in text
    assert 'CNP TEST' not in event['message'] and 'offers' not in event
    assert n.telegram_message(event,language,{})==event['message']
