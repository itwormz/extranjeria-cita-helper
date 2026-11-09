import json
from pathlib import Path
import pytest
from .timeline_reference import replay
DATA=json.loads((Path(__file__).parent/'fixtures/timeline.json').read_text())
@pytest.mark.parametrize('case',DATA['cases'],ids=lambda c:c['id'])
def test_python_timeline_projection(case):
    # JSON normalizes tuples used by restored_ranges into the persisted representation.
    assert json.loads(json.dumps(replay(case['input'])))==case['expected']
