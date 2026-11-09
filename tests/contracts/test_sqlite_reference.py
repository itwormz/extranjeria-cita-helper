import json
from pathlib import Path
import pytest
from .sqlite_reference import replay
DATA=json.loads((Path(__file__).parent/'fixtures/sqlite-migration.json').read_text())
@pytest.mark.parametrize('case',DATA['cases'],ids=lambda c:c['id'])
def test_existing_python_sqlite_migration_facts(case,tmp_path):
    assert replay(case['source'],tmp_path/'stats.json')==case['expected']
