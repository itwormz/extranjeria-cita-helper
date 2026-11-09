import json
from pathlib import Path
import pytest
from .writer_reference import replay
DATA=json.loads((Path(__file__).parent/'fixtures/statistics-writes.json').read_text())
@pytest.mark.parametrize('case',DATA['cases'],ids=lambda c:c['id'])
def test_python_statistics_writes(case,tmp_path):assert replay(case['input'],tmp_path/'statistics.json')==case['expected']
