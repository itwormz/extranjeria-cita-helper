import logging

from app.logging_utils import KeyValueFormatter, register_sensitive


def test_contact_fields_are_masked_in_structured_logs():
    record = logging.LogRecord('test', logging.INFO, '', 0, '', (), None)
    record.fields = {'contact_phone': '600000000', 'contact_email': 'person@example.invalid'}
    result = KeyValueFormatter().format(record)
    assert '600000000' not in result
    assert 'person@example.invalid' not in result
