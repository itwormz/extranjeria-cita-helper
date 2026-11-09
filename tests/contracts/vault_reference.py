"""Legacy manifest compatibility using an in-memory backend only."""
from app.desktop.notification_store import NotificationVault
from app.notifications import NotificationError
class Memory:
    def __init__(self,values):self.values=values.copy()
    def get_password(self,service,key):return self.values.get(key)
    def set_password(self,service,key,value):self.values[key]=value
    def delete_password(self,service,key):self.values.pop(key,None)
def replay(case):
    memory=Memory(case['entries']);vault=NotificationVault(memory)
    try:return dict(data=vault.get(case['reference']))
    except NotificationError:return dict(error='unreadable')
