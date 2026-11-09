import json
from datetime import datetime,timedelta,timezone
from app.desktop.statistics import ProfileStatistics

def replay(case,path):
    store=ProfileStatistics(path);profile='fixture';session='session';base=datetime(2026,10,30,12,tzinfo=timezone.utc)
    for step in case['steps']:
        at=base+timedelta(seconds=step['at']);kind=step['op']
        if kind=='start':store.start_session(profile,session,step.get('context',{}),at)
        elif kind=='end':store.end_session(profile,session,step.get('reason','stopped'),at)
        elif kind=='page':store.observe_page(profile,session,step['stage'],step.get('document','doc'),at,recognition='loading' if step['stage']=='loading' else 'unknown' if step['stage']=='unknown' else 'known',evidence=['operation_code'] if step.get('operation_code') else [],variant=step.get('variant',''))
        elif kind=='phase':store.phase(profile,session,step['mode'],at)
        elif kind=='heartbeat':store.heartbeat(profile,session,at)
        elif kind=='active':store.add_active(profile,step['seconds'],at=at,session_id=session,summary=False)
        elif kind=='counter':store.event(profile,step['kind'],step['document'],at=at,session_id=session,counted=step.get('counted',True),offices=step.get('offices',[]))
        elif kind=='enrich':store.save_booking_details(profile,session,step['document'],step['details'])
        else:raise ValueError('unknown fixture operation')
    result={}
    with store._db(write=False) as db:
        for table in ['events','episodes','sessions','active','activity_days']:
            result[table]=[]
            for row in db.execute('SELECT * FROM '+table):
                row=dict(row)
                for key in ['offices','evidence','context']:
                    if key in row:row[key]=json.loads(row[key])
                result[table].append(row)
    return result
