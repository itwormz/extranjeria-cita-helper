"""Stable facts from the existing journal; no filesystem or applicant data."""
from datetime import datetime, timezone
from unittest.mock import patch
from app.assist_journal import PageJournal
from app.booking_details import normalize_booking_details

def replay(steps):
    journal = PageJournal(lambda _row: None)
    for index, step in enumerate(steps):
        at = datetime.fromtimestamp((1792454400000 + index * 1000) / 1000, timezone.utc)
        class Clock(datetime):
            @classmethod
            def now(cls, tz=None):
                return at.astimezone(tz) if tz else at
        with patch('app.assist_journal.stamp', return_value=at.isoformat(timespec='milliseconds')), patch('app.assist_journal.datetime', Clock):
            if 'observe' in step:
                fields=step['observe'];journal.observe(document=fields['document'],stage=fields['stage'],page='https://site.test/same',title=fields['stage'],applicant_continue=fields.get('applicant_continue',False))
            elif 'event' in step:
                fields=dict(step['event']);event=fields.pop('event');journal.event(event,**fields)
            elif 'phase' in step:
                fields=step['phase'];journal.phase(fields['mode'],fields.get('remaining'), '')
            elif 'offices' in step:
                fields=step['offices'];journal.offices(fields['options'],fields.get('selected'))
            elif 'booking' in step:
                fields=step['booking'];journal.booking(fields['document'],normalize_booking_details(fields['details']))
            else:raise ValueError('unknown fixture operation')
    def millis(value):
        return None if value is None else int(datetime.fromisoformat(value).timestamp()*1000)
    return [dict(id=r['id'],time=millis(r['time']),ended_at=millis(r['ended_at']),event=r['event'],stage=r['stage'],document=r.get('document',''),status=r['status'],destination=r['destination'],submitted=r['submitted'],checked=r['checked'],offices=r['offices'],selected_office=r['selected_office'],applicant_continue=r.get('applicant_continue',False),booking_details=r.get('booking_details',{})) for r in journal.snapshot()]
