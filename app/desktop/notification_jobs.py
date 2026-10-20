"""Bounded background work for notification requests, with GUI-thread callbacks."""
import uuid
from PySide6.QtCore import QObject, Signal, QRunnable, QThreadPool
from ..notifications import NotificationError


class _Task(QRunnable):
    def __init__(self,jobs,key,work):
        super().__init__();self.jobs=jobs;self.key=key;self.work=work

    def run(self):
        try:result=(True,self.work())
        except NotificationError as exc:result=(False,str(exc))
        except Exception:result=(False,'Не удалось выполнить запрос уведомления')
        self.jobs.completed.emit(self.key,result)


class NotificationJobs(QObject):
    completed=Signal(str,object)

    def __init__(self,parent=None):
        super().__init__(parent);self.callbacks={};self.pool=QThreadPool(self)
        self.pool.setMaxThreadCount(3);self.completed.connect(self._done)

    def submit(self,work,callback):
        if len(self.callbacks)>=32:
            callback(False,'Очередь уведомлений заполнена');return
        key=uuid.uuid4().hex;self.callbacks[key]=callback
        self.pool.start(_Task(self,key,work))

    def _done(self,key,result):
        callback=self.callbacks.pop(key,None)
        if callback:callback(*result)
