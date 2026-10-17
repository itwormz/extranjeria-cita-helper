"""Bounded JSON Lines framing and a small command whitelist."""
import json


def encode_message(message):
    return (json.dumps(message, ensure_ascii=False, separators=(',', ':')) + '\n').encode('utf-8')


class JsonLinesDecoder:
    def __init__(self, limit=65536):
        self.buffer = b''
        self.limit = limit
        self.discarding = False

    def feed(self, data):
        rows = []
        if self.discarding:
            if b'\n' not in data:
                return rows
            data = data.split(b'\n', 1)[1]
            self.discarding = False
        self.buffer += data
        while b'\n' in self.buffer:
            line, self.buffer = self.buffer.split(b'\n', 1)
            try:
                if len(line) > self.limit:
                    raise ValueError()
                row = json.loads(line)
                if not isinstance(row, dict):
                    raise ValueError()
                rows.append(row)
            except (ValueError, UnicodeError):
                rows.append({'type': 'protocol_error'})
        if len(self.buffer) > self.limit:
            self.buffer = b''
            self.discarding = True
            rows.append({'type': 'protocol_error'})
        return rows


def validate_command(row):
    name = row.get('command')
    if name in {'stop', 'shutdown', 'restart', 'show_browser'}:
        return set(row) == {'command'}
    if name == 'pause':
        return set(row) == {'command', 'paused'} and type(row['paused']) is bool
    if name == 'preferences':
        return set(row) == {'command', 'diagnostics', 'sound'} and all(type(row[k]) is bool for k in ('diagnostics', 'sound'))
    return False
