from pathlib import Path
from unittest.mock import patch
from app import logging_utils as logs
from app.diagnostic_resources import script_url,script_reference
from app.desktop.startup_report import clean

def replay(case):
    if case['op']=='resource':return dict(url=script_url(case['text']),reference=script_reference(case['text']))
    logs._known_sensitive.clear();logs.register_sensitive(case.get('known',[]))
    try:
        if case['op']=='sanitize':return logs.sanitize(case['text'])
        with patch('app.desktop.startup_report.Path.home',return_value=Path('/Users/Fictional')):return clean(case['text'])
    finally:logs._known_sensitive.clear()
