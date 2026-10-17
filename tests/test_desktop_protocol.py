import pytest
from app.desktop.protocol import JsonLinesDecoder, encode_message, validate_command
from app.assist_control import AssistControl


def test_partial_utf8_lines_and_invalid_payload_do_not_leak():
    decoder = JsonLinesDecoder()
    wire = encode_message({'type': 'state', 'screen': 'Сессия'})
    assert decoder.feed(wire[:12]) == []
    assert decoder.feed(wire[12:]) == [{'type': 'state', 'screen': 'Сессия'}]
    assert decoder.feed(b'not-json-SECRET\n') == [{'type': 'protocol_error'}]
    assert decoder.feed(b'x' * 65537) == [{'type': 'protocol_error'}]
    assert decoder.feed(b'\n' + wire) == [{'type': 'state', 'screen': 'Сессия'}]


@pytest.mark.parametrize('message', [{'command':'shell'}, {'command':'pause','paused':'yes'}, {'command':'stop','secret':'PRIVATE'}])
def test_command_whitelist(message):
    assert validate_command(message) is False


def test_control_interruptions_and_preferences():
    control = AssistControl()
    control.set_diagnostics(False); control.set_sound(False)
    control.request_restart()
    assert control.restart_requested and not control.stop_requested
    assert not control.diagnostics_enabled and not control.sound_enabled
    control.clear_restart()
    assert not control.restart_requested
    control.request_stop()
    assert control.stop_requested
