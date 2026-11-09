"""Read queries from the real Python statistics implementation, synthetic inputs only."""
import json
from app.desktop.statistics import ProfileStatistics

def replay(case,path):
    store=ProfileStatistics(path)
    with store._db() as db:
        if case.get('baseline') is not None:
            db.execute('INSERT INTO baseline VALUES (?,?)',('fixture',json.dumps(case['baseline'])))
        if case.get('provenance') is not None:
            db.execute('INSERT INTO baseline_provenance VALUES (?,?,?)',('fixture',json.dumps(case['provenance']),case['migrated_at']))
        for interval in case.get('observed',[]):
            db.execute('INSERT INTO observed_active VALUES (?,?,?,?)',('fixture','observed',interval['start'],interval['end']))
        for i,event in enumerate(case.get('events',[])):
            db.execute('INSERT INTO events(profile,identity,kind,time,session,counted,offices) VALUES (?,?,?,?,?,?,?)',
                (event.get('profile','fixture'),str(i),event['kind'],event['time'],event.get('session','session'),event.get('counted',1),json.dumps(event.get('offices',[]))))
        for interval in case.get('active',[]):
            db.execute('INSERT INTO active(profile,session,start,end,seconds) VALUES (?,?,?,?,?)',('fixture','session',interval['start'],interval['end'],interval['seconds']))
    period=case['period'];now=case['now'];profile=case.get('profile','fixture')
    return dict(summary=store.get(profile,period,now),total=store.total(profile,period,now),offices=store.office_history(profile,period,now),
        known=store.known_offices(profile,period,now),last=store.last_office(profile,period,now),bounds=list(store._bounds(period,now)))
