"""Actual StatisticsSession + ProfileStatistics on an isolated synthetic DB."""
from types import SimpleNamespace
from app.desktop.statistics import ProfileStatistics, COUNTERS
from app.desktop.statistics_session import StatisticsSession

def replay(steps,path):
    store=ProfileStatistics(path);sessions={}
    for i,step in enumerate(steps):
        session=step.get('session','session');profile=step.get('profile','fixture')
        state=sessions.setdefault((profile,session),SimpleNamespace(statistics=store,profile_id=profile,session_id=session,_pass_seen=False,_error_kind=None))
        at=f'2026-10-20T00:00:{i:02d}+00:00'
        if 'observe' in step:
            event=step['observe'];StatisticsSession.observe(state,event['kind'],event['identity'],offices=event.get('offices',()),at=at)
        elif 'page' in step:
            StatisticsSession.record(state,'page',stage=step['page'],document=f'page-{i}',at=at)
        else:raise ValueError('unknown fixture operation')
    return {profile:dict(totals={key:store.get(profile)[key] for key in COUNTERS},known_offices=store.known_offices(profile)) for profile,_ in sessions}
