"""Current Python migration on an isolated synthetic DB, never user storage."""
import json
from unittest.mock import patch
from app.desktop import statistics as module

def replay(source,path):
    if source is not None:path.write_text(source,encoding='utf-8')
    store=module.ProfileStatistics(path)
    original=module._stamp
    try:
        with patch('app.desktop.statistics._stamp',side_effect=lambda value=None:'2026-10-29T12:00:00+00:00' if value is None else original(value)):
            with store._db() as db:
                baseline={r['profile']:json.loads(r['data']) for r in db.execute('SELECT * FROM baseline ORDER BY profile')}
                provenance={r['profile']:json.loads(r['data']) for r in db.execute('SELECT * FROM baseline_provenance ORDER BY profile')}
                metadata={r['key']:r['value'] for r in db.execute('SELECT * FROM metadata ORDER BY key')}
                schema={r['name']:[list(v) for v in db.execute('PRAGMA table_info('+r['name']+')')] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").fetchall()}
                indexes=[list(r) for r in db.execute("SELECT name,tbl_name,sql FROM sqlite_master WHERE type='index' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
        return dict(baseline=baseline,provenance=provenance,metadata=metadata,schema=schema,indexes=indexes,unchanged=source is None or path.read_text(encoding='utf-8')==source)
    except (ValueError,TypeError,OSError):return dict(error=True,unchanged=source is None or path.read_text(encoding='utf-8')==source)
