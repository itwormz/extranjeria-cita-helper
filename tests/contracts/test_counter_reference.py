import json
from pathlib import Path
import pytest
from .counter_reference import replay
DATA=json.loads((Path(__file__).parent/'fixtures/counters.json').read_text())
@pytest.mark.parametrize('case',DATA['cases'],ids=lambda c:c['id'])
def test_existing_python_counter_facts(case,tmp_path):
    assert replay(case['steps'],tmp_path/'stats.json')==case['expected']
