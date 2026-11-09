import json
from pathlib import Path
import pytest
from .diagnostic_reference import replay
DATA=json.loads((Path(__file__).parent/'fixtures/diagnostics.json').read_text())
@pytest.mark.parametrize('case',DATA['cases'],ids=lambda c:c['id'])
def test_python_diagnostic_contract(case):assert replay(case['input'])==case['expected']
