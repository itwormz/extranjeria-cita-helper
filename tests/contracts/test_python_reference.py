"""Golden action intents shared by the Python baseline and future Rust core."""
import json
from pathlib import Path

import pytest
from tests.contracts.python_reference import replay

FIXTURE = Path(__file__).parent / 'fixtures' / 'flow-actions.json'
CASES = json.loads(FIXTURE.read_text())['cases']


@pytest.mark.parametrize('case', CASES, ids=lambda case: case['id'])
def test_python_flow_matches_language_neutral_golden(case):
    assert replay(case) == case['expected']
