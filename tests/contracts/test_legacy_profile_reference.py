import json
from pathlib import Path
import pytest
from .legacy_profile_reference import replay
DATA=json.loads((Path(__file__).parent/'fixtures/legacy-profiles.json').read_text())
@pytest.mark.parametrize('case',DATA['cases'],ids=lambda c:c['id'])
def test_existing_python_legacy_profile_facts(case,tmp_path):
    assert replay(case['source'],tmp_path/'profiles.yml',DATA['catalog'],case.get('system_language','en'))==case['expected']
