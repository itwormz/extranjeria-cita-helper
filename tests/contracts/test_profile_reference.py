import json
from pathlib import Path
import pytest
from tests.contracts.profile_reference import validate

DATA=json.loads((Path(__file__).parent/'fixtures/profile-validation.json').read_text())

@pytest.mark.parametrize('case',DATA['cases'],ids=lambda case:case['id'])
def test_python_profile_validation_matches_golden(case):
    assert validate(case['draft'], DATA['catalog']) == case['expected']
