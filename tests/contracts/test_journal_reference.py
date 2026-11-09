import json
from pathlib import Path
import pytest
from .journal_reference import replay
DATA=json.loads((Path(__file__).parent/'fixtures/journal.json').read_text())
@pytest.mark.parametrize('case',DATA['cases'],ids=lambda c:c['id'])
def test_existing_python_journal_facts(case):
    assert replay(case['steps'])==case['expected']
