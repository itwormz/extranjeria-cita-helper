import json
from pathlib import Path
import pytest
from tests.contracts.recognition_reference import reference

DATA=json.loads((Path(__file__).parent/'fixtures/recognition.json').read_text())
@pytest.mark.parametrize('case',DATA['cases'],ids=lambda case:case['id'])
def test_python_recognition_matches_visible_facts_golden(case):
    assert reference(case['snapshot']) == case['expected']
