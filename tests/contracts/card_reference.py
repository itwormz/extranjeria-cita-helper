"""Record the draw calls of the Python 0.3.6 achievement card (app/desktop/achievement_card.py).

Usage: QT_QPA_PLATFORM=offscreen uv run python -m tests.contracts.card_reference > fixtures/achievement-card-0.3.6.json

`_Card.box/text/line/icon` are replaced by recorders; the painter is wrapped so the rotated confetti and
the sparkle strokes of the header are recorded too. Font metrics (the only layout input that depends on
Qt) are recorded as per-character advances of the value font.
"""
import json
import os
import sys

os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')

from PySide6.QtGui import QFont, QFontMetricsF  # noqa: E402
from PySide6.QtWidgets import QApplication  # noqa: E402

from app.desktop import achievement_card as module  # noqa: E402
from app.desktop.theme import load_fonts, theme_colors  # noqa: E402

COLOR_KEYS = ('bg', 'surface', 'line', 'success', 'text', 'muted', 'accent_text', 'celebration_gold')


def color(value):
    return None if value is None else str(value).lower()


class RecordingPainter:
    """Delegates to the real painter; records state changes made outside box/text/line/icon."""

    def __init__(self, real, card):
        self._real = real
        self._card = card
        self._pen = None

    def __getattr__(self, name):
        attribute = getattr(self._real, name)
        if name not in {'save', 'restore', 'translate', 'rotate', 'setPen', 'drawLine'} or not callable(attribute):
            return attribute

        def call(*args):
            result = attribute(*args)
            if self._card._depth == 0:
                if name == 'setPen':
                    pen = args[0]
                    self._pen = (color(pen.color().name()), pen.width()) if hasattr(pen, 'color') else None
                elif name == 'drawLine':
                    start, end = args
                    self._card._ops.append(dict(op='stroke', color=self._pen[0], width=self._pen[1],
                                                x1=start.x(), y1=start.y(), x2=end.x(), y2=end.y()))
                elif name in {'translate'}:
                    self._card._ops.append(dict(op='translate', x=args[0], y=args[1]))
                elif name == 'rotate':
                    self._card._ops.append(dict(op='rotate', angle=args[0]))
                else:
                    self._card._ops.append(dict(op=name))
            return result
        return call


def install():
    original_init = module._Card.__init__
    originals = {name: getattr(module._Card, name) for name in ('box', 'text', 'line')}

    def init(self, language, dark):
        original_init(self, language, dark)
        self._ops = []
        self._depth = 0
        self.painter = RecordingPainter(self.painter, self)

    def wrap(name, record):
        original = originals.get(name)

        def method(self, *args, **kwargs):
            self._ops.append(record(self, *args, **kwargs))
            if original is not None:
                self._depth += 1
                try:
                    original(self, *args, **kwargs)
                finally:
                    self._depth -= 1
        return method

    def box(self, x, y, w, h, fill, border=None, radius=10):
        return dict(op='box', x=x, y=y, w=w, h=h, color=color(fill), border=color(border), radius=radius)

    def text(self, x, y, w, h, value, size=24, fill=None, bold=False, right=False, wrap=False):
        return dict(op='text', x=x, y=y, w=w, h=h, value=value, size=size, color=color(fill or self.colors['text']),
                    bold=bold, right=right, wrap=wrap)

    def line(self, x, y, w):
        return dict(op='line', x=x, y=y, w=w, color=color(self.colors['line']))

    def icon(self, name, x, y, size=26, fill=None):
        return dict(op='icon', name=name, x=x, y=y, size=size, color=color(fill or self.colors['text']))

    module._Card.__init__ = init
    module._Card.box = wrap('box', box)
    module._Card.text = wrap('text', text)
    module._Card.line = wrap('line', line)
    module._Card.icon = wrap('icon', icon)


def scenarios():
    full = {"checks": 3645, "started_at": "2026-09-27T12:00:00+00:00", "active_seconds": 174720, "days": 17,
            "offices_seen": 4, "unique_offices": 2, "booking_screens": 1, "banners": 120, "no_slots": 33,
            "server_errors": 3, "rate_limited": 1, "blocked": 0, "flow_passes": 50, "cod_oper_count": 2,
            "actions_performed": 900, "pages_processed": 2000, "first_available_seconds": 8040,
            "first_office_seconds": 45, "first_office_active_quality": "exact",
            "first_booking_seconds": 7200, "first_booking_active_quality": "minimum",
            "effort_started_at": "2026-09-27T12:00:00+00:00", "name": "PRIVATE PERSON"}
    legacy = {"checks": 48, "active_seconds": 0, "offices_seen": 0, "server_errors": 0,
              "legacy_unknown_fields": ["active_seconds", "server_errors", "banners", "cod_oper_count", "days"]}
    return [
        ('full', full),
        ('unknown_legacy_values', legacy),
        ('minimum_quality_both', {**full, "first_office_active_quality": "minimum"}),
        ('zeros_and_sub_minute_events', {**{k: 0 for k in ("checks", "offices_seen", "unique_offices", "booking_screens", "banners", "no_slots", "server_errors", "rate_limited", "blocked", "flow_passes", "cod_oper_count")}, "days": 0, "active_seconds": 59, "first_office_seconds": 5, "first_office_active_quality": "exact", "first_booking_seconds": 59.5, "first_booking_active_quality": "exact", "effort_started_at": "x", "actions_performed": 0, "pages_processed": 0, "checks": 1}),
        ('very_large_numbers', {**full, "checks": 123456789012, "banners": 98765432109, "no_slots": 1234567, "pages_processed": 7654321098}),
    ]


def run():
    application = QApplication.instance() or QApplication([])
    load_fonts(application)
    install()
    font = QFont('IBM Plex Sans')
    font.setPixelSize(24)
    font.setWeight(QFont.Weight.DemiBold)
    metrics = QFontMetricsF(font)
    advances = {character: metrics.horizontalAdvance(character) for character in '0123456789.—'}
    cases = []
    for name, statistics in scenarios():
        languages = ('ru', 'en', 'es') if name == 'full' else ('en',)
        themes = (True, False) if name == 'full' else (True,)
        for language in languages:
            for dark in themes:
                recorded = {}
                original = module._Card.__init__

                def capture(self, language_, dark_, original=original, recorded=recorded):
                    original(self, language_, dark_)
                    recorded['card'] = self

                module._Card.__init__ = capture
                try:
                    achievement = {'statistics': statistics, 'recorded_at': '2026-10-14T12:00:00+00:00'}
                    image = module.render_achievement_card(achievement, language, dark)
                finally:
                    module._Card.__init__ = original
                card = recorded['card']
                cases.append(dict(id=f"{name}_{language}_{'dark' if dark else 'light'}", scenario=name,
                                  language=language, dark=dark, statistics=statistics,
                                  size=[image.width(), image.height()], background=color(card.colors['bg']),
                                  ops=card._ops))
    colors = {theme: {key: color(value) for key, value in theme_colors(theme == 'dark').items() if key in COLOR_KEYS}
              for theme in ('dark', 'light')}
    return dict(schema=1, source='v0.3.6', colors=colors, advances=advances, cases=cases)


if __name__ == '__main__':
    json.dump(run(), sys.stdout, ensure_ascii=False, indent=1, sort_keys=True)
    sys.stdout.write('\n')
