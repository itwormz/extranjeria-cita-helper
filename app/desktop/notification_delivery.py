"""GUI-side state routing. Delivery never drives or inspects the browser."""
import copy
from PySide6.QtCore import QObject, Signal
from .notification_jobs import NotificationJobs
from .notification_store import NotificationVault
from ..notifications import settings,EventRouter,make_event,Telegram,webhook_request,send_request,state_event,NotificationError,telegram_message


class NotificationDelivery(QObject):
    result=Signal(str,str)

    def __init__(self,store,parent=None):
        super().__init__(parent);self.store=store;self.jobs=NotificationJobs(self);self.vault=NotificationVault()
        self.router=EventRouter();self.telegram_router=EventRouter();self.profile_id=None;self.generation=0;self.current_kind=None;self.closed=False

    def observe(self,profile_id,state,language):
        if not profile_id or self.closed:return
        if self.profile_id!=profile_id:
            self.router=EventRouter();self.telegram_router=EventRouter();self.profile_id=profile_id
        event=self.router.observe(state)
        kind=state_event(state)
        if kind!=self.current_kind:
            self.generation+=1;self.current_kind=kind
        telegram_event=None if kind=='booking' and state.get('booking_details_pending') else self.telegram_router.observe(state)
        if not event and not telegram_event:return
        try:raw=self.store.raw_profile(profile_id)
        except (ValueError,StopIteration):return
        config=settings(raw.get('notifications',{}))
        payload=make_event(event or telegram_event,raw.get('label',''),raw.get('request',{}).get('province',''),language)
        generation=self.generation
        current=lambda:not self.closed and self.generation==generation and self.profile_id==profile_id
        message=telegram_message(payload,language,state.get('booking_details'))
        for channel in ('telegram','webhook'):
            if not (telegram_event if channel=='telegram' else event):continue
            connection=copy.deepcopy(config[channel])
            if not connection['enabled'] or (telegram_event if channel=='telegram' else event) not in connection.get('events',[]):continue
            def deliver(channel=channel,connection=connection):
                if not current():raise NotificationError('Уведомление устарело')
                secret=self.vault.get(connection.get('secret_ref',''))
                if channel=='telegram':
                    client=Telegram(secret.get('token',''))
                    failures=0;accepted=0
                    for recipient in connection.get('recipients',[]):
                        if not current():raise NotificationError('Уведомление устарело')
                        try:client.send(recipient['id'],message,current);accepted+=1
                        except NotificationError:failures+=1
                    if failures:return 'Уведомление доставлено не всем получателям' if accepted else 'Не удалось доставить уведомление'
                    return 'Сообщение принято Telegram'
                code,_,elapsed=send_request(webhook_request(secret,payload),current=current)
                return f'HTTP {code} · {elapsed} ms'
            def done(ok,value,channel=channel):
                if current():self.result.emit(channel,value)
            self.jobs.submit(deliver,done)

    def reset(self):
        self.generation+=1;self.router=EventRouter();self.telegram_router=EventRouter();self.current_kind=None

    def stop(self):self.closed=True;self.reset()
