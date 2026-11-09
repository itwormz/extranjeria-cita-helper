import json
from pathlib import Path
import pytest
from .export_reference import replay
DATA=json.loads((Path(__file__).parent/'fixtures/exports.json').read_text())
@pytest.mark.parametrize('case',DATA['cases'],ids=lambda c:c['id'])
def test_python_export_contract(case,tmp_path):assert replay(case['input'],tmp_path/'fixture.ics')==case['expected']
