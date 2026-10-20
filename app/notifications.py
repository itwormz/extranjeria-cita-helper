"""Profile notification contracts and bounded HTTP delivery; never log request data."""
import copy
from datetime import datetime, timezone
from email.utils import parsedate_to_datetime
import json
import re
import secrets
import ssl
import time
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode, urlsplit, urlunsplit
from urllib.request import Request, HTTPRedirectHandler, build_opener, HTTPSHandler
import uuid

import certifi

EVENTS = ('auth', 'office', 'booking', 'error')
EVENT_LABELS = {'auth':'Авторизация', 'office':'Выбор офиса', 'booking':'Выбор ситы', 'error':'Остановка с ошибкой'}
VARIABLES = ('message', 'event', 'profile', 'province', 'timestamp', 'event_id')


class NotificationError(ValueError):
    """Only fixed, translatable messages may cross this boundary."""


def defaults():
    return {'telegram': {'enabled':False, 'events':list(EVENTS), 'recipients':[], 'secret_ref':''},
            'webhook': {'enabled':False, 'events':list(EVENTS), 'secret_ref':''}}


def settings(raw):
    result=defaults()
    for channel in result:
        source=raw.get(channel,{}) if isinstance(raw,dict) else {}
        if isinstance(source,dict):result[channel].update(copy.deepcopy(source))
    return result


def state_event(state):
    if state.get('failed'):return 'error'
    if state.get('finished') or state.get('mode')=='finished':return None
    if state.get('connection_lost') or state.get('paused'):return None
    if state.get('booking_handoff'):return 'booking'
    if state.get('mode')!='manual':return None
    screen=state.get('screen','')
    if screen=='Выбор офиса':return 'office'
    if screen in {'Выбор ситы','Капча'}:return 'booking'
    if screen in {'Авторизация сертификатом','Cl@ve Móvil','Cl@ve Permanente','Авторизация'}:return 'auth'
    return None


class EventRouter:
    def __init__(self):self.last=None

    def observe(self,state):
        if not state.get('failed') and (state.get('connection_lost') or state.get('paused')):return None
        event=state_event(state)
        key=(state.get('run_id'),event) if event else None
        previous=self.last;self.last=key
        return event if key and key!=previous else None


def make_event(kind,profile,province,language):
    from .desktop.i18n import translate
    title={'auth':'Проверить авторизацию в Camoufox','office':'Требуется выбрать офис',
           'booking':'Открыт выбор ситы','error':'Поиск завершился с ошибкой','test':'Тестовое уведомление'}[kind]
    action={'auth':'Проверить браузер и завершить вход при необходимости.',
            'office':'Выбрать офис в Camoufox.',
            'booking':'Выбрать дату и время, заполнить CAPTCHA и подтвердить запись в Camoufox.',
            'error':'Открыть приложение и проверить диагностику.',
            'test':'Подключение уведомлений проверено.'}[kind]
    t=lambda text:translate(text,language)
    return dict(schema_version=1,event_id=str(uuid.uuid4()),event=kind,
                timestamp=datetime.now(timezone.utc).isoformat(),profile=str(profile),province=str(province),
                action_required=kind!='test',message=f'{t(title)}\n\n{t("Профиль")}: {profile}\n{t("Провинция")}: {province}\n\n{t(action)}\n\nCita Extranjeria Helper')


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self,*args,**kwargs):return None


def urlopen(request,timeout=10):
    # No credential forwarding across redirects, and no insecure TLS fallback.
    context=ssl.create_default_context(cafile=certifi.where())
    return build_opener(NoRedirect(),HTTPSHandler(context=context)).open(request,timeout=timeout)


def retry_seconds(value,attempt):
    try:
        seconds=float(value)
    except (TypeError,ValueError):
        try:seconds=(parsedate_to_datetime(value)-datetime.now(timezone.utc)).total_seconds()
        except (TypeError,ValueError,OverflowError):seconds=2**attempt
    return max(1,seconds)


def send_request(request,*,attempts=3,current=lambda:True):
    started=time.monotonic()
    for attempt in range(attempts):
        if not current():raise NotificationError('Уведомление устарело')
        delay=2**attempt
        try:
            with urlopen(request,timeout=10) as response:
                data=response.read(256*1024+1)
                if len(data)>256*1024:raise NotificationError('Слишком большой ответ получателя')
                return response.status,data,round((time.monotonic()-started)*1000)
        except HTTPError as exc:
            status=exc.code;delay=retry_seconds(exc.headers.get('Retry-After'),attempt)
            exc.close()
            if delay>60:raise NotificationError(f'HTTP {status}') from None
            if status!=429 and status<500:raise NotificationError(f'HTTP {status}') from None
            if attempt==attempts-1:raise NotificationError(f'HTTP {status}') from None
        except (URLError,OSError,TimeoutError,ssl.SSLError):
            if attempt==attempts-1:raise NotificationError('Не удалось доставить уведомление') from None
        end=time.monotonic()+delay
        while time.monotonic()<end:
            if not current():raise NotificationError('Уведомление устарело')
            time.sleep(min(.2,max(0,end-time.monotonic())))
    raise NotificationError('Не удалось доставить уведомление')


def request_json(url,data,*,attempts=1):
    request=Request(url,data=json.dumps(data).encode(),headers={'Content-Type':'application/json'},method='POST')
    _,body,_=send_request(request,attempts=attempts)
    try:return json.loads(body)
    except (ValueError,UnicodeError):raise NotificationError('Некорректный ответ получателя') from None


def expand(value,event):
    def replace(match):
        key=match.group(1)
        if key not in VARIABLES:raise NotificationError('Неизвестная переменная шаблона')
        return str(event[key])
    return re.sub(r'\{\{\s*(\w+)\s*\}\}',replace,str(value))


def custom_body(template,event):
    try:raw=json.loads(template)
    except (ValueError,TypeError):raise NotificationError('Некорректный JSON') from None
    def walk(value):
        if isinstance(value,str):return expand(value,event)
        if isinstance(value,list):return [walk(v) for v in value]
        if isinstance(value,dict):return {k:walk(v) for k,v in value.items()}
        return value
    return walk(raw)


def validate_webhook(config):
    try:
        url=config.get('url','');parts=urlsplit(url)
        if parts.scheme!='https' or not parts.hostname or parts.username or parts.password or parts.fragment or any(c.isspace() for c in url):raise ValueError()
        parts.port
    except (ValueError,TypeError):raise NotificationError('Указать HTTPS URL без логина и пароля') from None
    if config.get('method','POST') not in {'GET','POST'}:raise NotificationError('Выбрать GET или POST')
    names=set()
    for row in config.get('headers',[]):
        name=row.get('name','');value=row.get('value','')
        if not re.fullmatch(r"[!#$%&'*+.^_`|~0-9A-Za-z-]+",name) or name.lower() in {'host','content-length','transfer-encoding','connection'} or name.lower() in names or any(c in value for c in '\r\n\0'):
            raise NotificationError('Проверить названия и значения заголовков')
        try:value.encode('latin-1')
        except UnicodeError:raise NotificationError('Заголовки должны содержать символы Latin-1') from None
        names.add(name.lower())
    if config.get('body_mode','standard') not in {'standard','custom'}:raise NotificationError('Некорректный формат тела запроса')
    event=make_event('test','Demo','Barcelona','en')
    for row in config.get('query',[]):
        if not row.get('name','').strip():raise NotificationError('Указать название параметра')
        expand(row.get('value',''),event)
    if config.get('method','POST')=='POST' and config.get('body_mode')=='custom':custom_body(config.get('body',''),event)


def webhook_request(config,event):
    validate_webhook(config)
    parts=urlsplit(config['url']);query=parts.query
    extra=urlencode([(r['name'],expand(r.get('value',''),event)) for r in config.get('query',[])])
    url=urlunsplit((parts.scheme,parts.netloc,parts.path,'&'.join(v for v in (query,extra) if v),''))
    headers={r['name']:r.get('value','') for r in config.get('headers',[])}
    body=None;method=config.get('method','POST')
    if method=='POST':
        value=custom_body(config.get('body',''),event) if config.get('body_mode')=='custom' else event
        body=json.dumps(value,ensure_ascii=False).encode('utf-8')
        if not any(k.lower()=='content-type' for k in headers):headers['Content-Type']='application/json'
    return Request(url,data=body,headers=headers,method=method)


def recipient_from_chat(chat):
    return dict(id=str(chat['id']),label=chat.get('title') or ' '.join(v for v in (chat.get('first_name'),chat.get('last_name')) if v) or str(chat['id']),
                type=chat.get('type','private'),username=chat.get('username',''))


class Pairing:
    def __init__(self,username,*,now=None):
        self.username=username;self.code=secrets.token_urlsafe(18)
        self.created=time.monotonic() if now is None else now
        self.offset=None

    def link(self,kind):return f'https://t.me/{self.username}?{"startgroup" if kind=="group" else "start"}={self.code}'

    def match(self,updates,kind,*,now=None):
        if (time.monotonic() if now is None else now)-self.created>120:return None
        for update in updates:
            self.offset=max(self.offset or 0,int(update.get('update_id',0))+1)
            message=update.get('message',{});text=message.get('text','');chat=message.get('chat',{})
            types={'group','supergroup'} if kind=='group' else {'private'}
            if text not in {f'/start {self.code}',f'/start@{self.username} {self.code}'} or chat.get('type') not in types:continue
            return recipient_from_chat(chat)
        return None


class Telegram:
    def __init__(self,token):
        if not re.fullmatch(r'\d{5,}:[A-Za-z0-9_-]+',token):raise NotificationError('Проверить токен бота')
        self.token=token

    def call(self,method,data=None):
        result=request_json(f'https://api.telegram.org/bot{self.token}/{method}',data or {})
        if not isinstance(result,dict) or not result.get('ok'):raise NotificationError('Telegram отклонил запрос')
        return result['result']

    def check(self):
        result=self.call('getMe')
        if not isinstance(result,dict) or not re.fullmatch(r'[A-Za-z0-9_]{5,}',result.get('username','')):raise NotificationError('Некорректный ответ получателя')
        return result

    def begin_pairing(self):
        if self.call('getWebhookInfo').get('url'):raise NotificationError('Бот используется другим сервисом. Ввести Chat ID вручную или создать отдельного бота.')
        return Pairing(self.check()['username'])

    def poll(self,pair,kind):
        if time.monotonic()-pair.created>120:raise NotificationError('Ссылка подключения истекла. Открыть бота заново.')
        data={'timeout':0,'limit':100}
        if pair.offset is not None:data['offset']=pair.offset
        return pair.match(self.call('getUpdates',data),kind)

    def chat(self,chat_id):
        if not re.fullmatch(r'-?\d+|@[A-Za-z0-9_]{5,}',str(chat_id)):raise NotificationError('Указать Chat ID или @имя канала')
        chat=self.call('getChat',{'chat_id':chat_id})
        return recipient_from_chat(chat)

    def send(self,chat_id,message,current=lambda:True):
        request=Request(f'https://api.telegram.org/bot{self.token}/sendMessage',data=json.dumps({'chat_id':chat_id,'text':message,'link_preview_options':{'is_disabled':True}}).encode(),headers={'Content-Type':'application/json'},method='POST')
        _,body,elapsed=send_request(request,current=current)
        try:accepted=json.loads(body).get('ok')
        except (ValueError,AttributeError):accepted=False
        if not accepted:raise NotificationError('Telegram отклонил запрос')
        return elapsed
