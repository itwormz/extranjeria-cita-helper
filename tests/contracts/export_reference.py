from datetime import datetime,timezone
from unittest.mock import patch
from app.desktop import achievement_exports as e
from app.desktop.achievements import snapshot

def replay(case,path):
    if case['op']=='snapshot':return snapshot(case['statistics'])
    if case['op']=='share':return e.share_url(case['network'])
    value=case['achievement'];language=case.get('language','en')
    if case['op']=='public':
        labels,days,rows=e._public(value,language)
        return dict(post=e.public_post(value,language,case.get('detailed',True)),duration=e.public_duration(value,language),card=dict(title=labels[0],duration=e.public_duration(value,language),rows=[list(row) for row in rows],credit=e._credit(language),width=1200,height=900,top_strip_height=7))
    try:
        with patch('app.desktop.achievement_exports.datetime',wraps=datetime) as clock:
            clock.now.return_value=datetime(2026,10,30,12,tzinfo=timezone.utc)
            e.write_calendar(value,path)
        return dict(calendar=path.read_bytes().decode(),google=e.google_calendar_url(value))
    except ValueError:return dict(error='invalid')
