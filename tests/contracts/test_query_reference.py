import json
from pathlib import Path
import pytest
from .query_reference import replay
DATA=json.loads((Path(__file__).parent/'fixtures/statistics-queries.json').read_text())
@pytest.mark.parametrize('case',DATA['cases'],ids=lambda c:c['id'])
def test_existing_python_statistics_queries(case,tmp_path):
    assert replay(case['input'],tmp_path/'statistics.json')==case['expected']
