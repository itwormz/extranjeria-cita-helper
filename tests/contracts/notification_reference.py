import json
from urllib.parse import urlsplit,urlunsplit
from app import notifications as n

def replay(case):
    op=case['op']
    if op=='router':
        router=n.EventRouter();return [router.observe(state) for state in case['states']]
    event=n.make_event(case.get('kind','booking'),'Demo','Barcelona',case.get('language','en'))
    event.update(event_id='fixture-id',timestamp='2026-10-30T12:00:00+00:00')
    if op=='message':return dict(event=event,text=n.telegram_message(event,case.get('language','en'),case.get('details')))
    if op=='webhook':
        try:
            request=n.webhook_request(case['config'],event)
            parts=urlsplit(request.full_url);url=urlunsplit((parts.scheme,parts.netloc,parts.path or '/',parts.query,''))
            return dict(method=request.method,url=url,headers={key.lower():value for key,value in request.header_items()},body=json.loads(request.data) if request.data is not None else None)
        except n.NotificationError:return dict(error='invalid')
    raise ValueError('unknown fixture')
