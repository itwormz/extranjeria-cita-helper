"""Public booking offers only: never filters, identifiers or whole-page text."""
import asyncio
from types import SimpleNamespace
from unittest.mock import AsyncMock
import pytest
from app.booking_details import (
    booking_lines, normalize_booking_details, parse_offers, read_booking_details,
)


def test_only_explicit_valid_radio_offers_are_parsed_and_deduplicated():
    assert parse_offers(['CITA 1 Día: 28/10/2026 Hora:12:30',
        'CITA 2 Día: 29/10/2026 Hora:12:06','CITA 1 Día: 28/10/2026 Hora:12:30',
        'a partir de las 09:00','Día: 31/02/2026 Hora:12:30','Día: 28/10/2026 Hora:25:30'])==[
        {'date':'2026-10-28','time':'12:30'},{'date':'2026-10-29','time':'12:06'}]


def test_read_failure_or_timeout_keeps_generic_fallback():
    async def slow(*args):await asyncio.sleep(1)
    for read in (AsyncMock(side_effect=RuntimeError('navigation')),slow):
        source=SimpleNamespace(evaluate=read)
        assert asyncio.run(read_booking_details(source,timeout=.01))=={}


def test_bounded_read_uses_only_offers_and_public_office():
    source=SimpleNamespace(evaluate=AsyncMock(return_value=['Día: 28/10/2026 Hora:12:30']))
    details=asyncio.run(read_booking_details(source,office='CNP TEST, CALLE TEST, 1'))
    assert details=={'office':'CNP TEST, CALLE TEST, 1','offers':[{'date':'2026-10-28','time':'12:30'}]}


@pytest.mark.parametrize('labels', [[], ['a partir de las 09:00'], ['Día: 31/02/2026 Hora:12:30']])
def test_unsupported_or_invalid_offers_do_not_return_office_only_details(labels):
    source = SimpleNamespace(evaluate=AsyncMock(return_value=labels))
    assert asyncio.run(read_booking_details(source, office='CNP TEST, CALLE TEST, 1')) == {}


@pytest.mark.parametrize('labels', [None, {}, 'Día: 28/10/2026 Hora:12:30'])
def test_non_list_offer_payload_is_ignored(labels):
    assert parse_offers(labels) == []


def test_malformed_labels_do_not_hide_later_valid_offers():
    labels = [None, 'search filters only', 'Día: 31/02/2026 Hora:12:30',
              'DÍA: 28/10/2026   HORA: 12:30']
    assert parse_offers(labels) == [{'date': '2026-10-28', 'time': '12:30'}]


def test_offer_output_is_bounded_even_when_more_slots_are_available():
    labels = [f'Día: 28/10/2026 Hora:12:{minute:02d}' for minute in range(12)]
    assert parse_offers(labels) == [
        {'date': '2026-10-28', 'time': f'12:{minute:02d}'} for minute in range(8)
    ]


def test_labels_outside_the_capture_limits_are_not_interpreted_as_offers():
    label = 'Día: 28/10/2026 Hora:12:30'
    assert parse_offers(['not an offer'] * 32 + [label]) == []
    # The final minute digit lies beyond the maximum captured label length.
    assert parse_offers(['x' * (241 - len(label)) + label]) == []


@pytest.mark.parametrize('office', ['', '   ', None, 123])
def test_missing_or_invalid_office_does_not_discard_public_slots(office):
    source = SimpleNamespace(evaluate=AsyncMock(return_value=['Día: 28/10/2026 Hora:12:30']))
    assert asyncio.run(read_booking_details(source, office=office)) == {
        'offers': [{'date': '2026-10-28', 'time': '12:30'}]
    }


def test_default_office_is_absent_and_long_office_is_bounded():
    source = SimpleNamespace(evaluate=AsyncMock(return_value=['Día: 28/10/2026 Hora:12:30']))
    assert 'office' not in asyncio.run(read_booking_details(source))
    details = asyncio.run(read_booking_details(source, office='  ' + 'A' * 400 + '  '))
    assert details['office'] == 'A' * 320


def test_read_timeout_cancels_the_pending_dom_read():
    async def scenario():
        cancelled = asyncio.Event()

        async def pending(_script):
            try:
                await asyncio.Event().wait()
            finally:
                cancelled.set()

        # The outer deadline detects a missing inner timeout without hanging pytest.
        details = await asyncio.wait_for(
            read_booking_details(SimpleNamespace(evaluate=pending), timeout=.01), .5
        )
        assert details == {}
        assert cancelled.is_set()

    asyncio.run(scenario())


@pytest.mark.parametrize('raw', [None, [], 'invalid', {'offers': None}, {'offers': 123}])
def test_invalid_history_payloads_normalize_to_empty_details(raw):
    assert normalize_booking_details(raw) == {}


def test_normalization_skips_bad_rows_without_losing_later_public_offers():
    raw = {
        'office': '  CNP   TEST  ',
        'captcha': 'private',
        'confirmation': 'private',
        'offers': [None, {'date': '2026-02-31', 'time': '12:30'},
                   {'date': '2026-10-28', 'time': '25:00'},
                   {'date': '2026-10-28', 'time': None},
                   {'date': '2026-10-28', 'time': '12:30', 'token': 'private'},
                   {'date': '2026-10-28', 'time': '12:30'}],
    }
    assert normalize_booking_details(raw) == {
        'office': 'CNP TEST', 'offers': [{'date': '2026-10-28', 'time': '12:30'}]
    }
    assert raw['office'] == '  CNP   TEST  '
    assert raw['offers'][-2]['token'] == 'private'


@pytest.mark.parametrize('office', ['   ', None, 123])
def test_invalid_office_is_not_retained_in_history(office):
    assert normalize_booking_details({'office': office}) == {}


def test_normalized_history_and_message_output_remain_bounded():
    rows = [{'date': '2026-10-28', 'time': f'12:{minute:02d}'} for minute in range(12)]
    result = normalize_booking_details({'office': 'A' * 400, 'offers': rows})
    assert result == {'office': 'A' * 320, 'offers': rows[:8]}
    assert booking_lines({'offers': rows[:1]}) == ['28.10.2026 — 12:00']
    assert booking_lines({'office': 'CNP TEST'}) == ['CNP TEST']
    assert booking_lines(None) == []
