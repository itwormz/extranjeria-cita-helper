"""Public booking offers only: never filters, identifiers or whole-page text."""
import asyncio
from types import SimpleNamespace
from unittest.mock import AsyncMock
import pytest
from app.booking_details import parse_offers, read_booking_details


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
