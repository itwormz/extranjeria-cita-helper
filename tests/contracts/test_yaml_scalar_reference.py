import json,yaml
from pathlib import Path
import pytest
DATA=json.loads((Path(__file__).parent/'fixtures/yaml-scalars.json').read_text())
@pytest.mark.parametrize('case',DATA['cases'],ids=lambda c:c['id'])
def test_existing_python_yaml_scalar_schema(case):
    try:
        value=yaml.safe_load(case['source']);actual=dict(kind=type(value).__name__,text=str(value))
    except (ValueError,yaml.YAMLError):actual=dict(error=True)
    assert actual==case['expected']
