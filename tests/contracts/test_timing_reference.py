from dataclasses import asdict
import json
from pathlib import Path
import pytest
from app.timings import parse_timings
DATA=json.loads((Path(__file__).parent/'fixtures/timings.json').read_text())
@pytest.mark.parametrize('case',DATA['cases'],ids=lambda case:case['id'])
def test_python_timings_match_golden(case):
    if case['expected']['valid']:
        assert asdict(parse_timings(case['input'])) == {key:tuple(value) if isinstance(value,list) else value for key,value in case['expected']['value'].items()}
    else:
        with pytest.raises((ValueError,TypeError)): parse_timings(case['input'])
