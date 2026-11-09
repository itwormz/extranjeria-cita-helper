import json
from pathlib import Path
import pytest
from app.booking_details import parse_offers, normalize_booking_details, booking_lines

DATA=json.loads((Path(__file__).parent/'fixtures/booking-offers.json').read_text())
@pytest.mark.parametrize('case',DATA['cases'],ids=lambda case:case['id'])
def test_python_booking_details_match_golden(case):
    functions={'parse':parse_offers,'normalize':normalize_booking_details,'lines':booking_lines}
    assert functions[case['operation']](case['input']) == case['expected']
