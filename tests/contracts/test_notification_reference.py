import json
from pathlib import Path
import pytest
from .notification_reference import replay
DATA=json.loads((Path(__file__).parent/'fixtures/notifications.json').read_text())
@pytest.mark.parametrize('case',DATA['cases'],ids=lambda c:c['id'])
def test_python_notification_contract(case):assert replay(case['input'])==case['expected']
