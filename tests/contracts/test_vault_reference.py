import json
from pathlib import Path
import pytest
from .vault_reference import replay
DATA=json.loads((Path(__file__).parent/'fixtures/vault.json').read_text())
@pytest.mark.parametrize('case',DATA['cases'],ids=lambda c:c['id'])
def test_python_vault_compatibility(case):assert replay(case['input'])==case['expected']
