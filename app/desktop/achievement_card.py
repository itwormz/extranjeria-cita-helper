"""Public achievement image: immutable search statistics, never booking details."""
from PySide6.QtCore import QByteArray, QPointF, QRectF, Qt
from PySide6.QtGui import QColor, QFont, QFontMetricsF, QImage, QPainter, QPen
from PySide6.QtSvg import QSvgRenderer
from PySide6.QtWidgets import QApplication

from .achievements import snapshot
from .i18n import translate
from .icon_paths import ICON_PATHS
from .theme import load_fonts, theme_colors


class _Card:
    def __init__(self, language, dark):
        self.language = language
        self.colors = theme_colors(dark)
        self.image = QImage(1200, 1240, QImage.Format.Format_ARGB32)
        self.image.fill(QColor(self.colors['bg']))
        self.painter = QPainter(self.image)
        self.painter.setRenderHint(QPainter.RenderHint.Antialiasing)

    def t(self, source):
        return translate(source, self.language)

    def box(self, x, y, w, h, color, border=None, radius=10):
        self.painter.setBrush(QColor(color))
        self.painter.setPen(QPen(QColor(border), 1) if border else Qt.PenStyle.NoPen)
        self.painter.drawRoundedRect(QRectF(x, y, w, h), radius, radius)

    def text(self, x, y, w, h, value, size=24, color=None, bold=False, right=False, wrap=False):
        rect = QRectF(x, y, w, h)
        flags = (Qt.AlignmentFlag.AlignRight if right else Qt.AlignmentFlag.AlignLeft) | Qt.AlignmentFlag.AlignVCenter
        if wrap:
            flags |= Qt.TextFlag.TextWordWrap
        font = QFont('IBM Plex Sans')
        font.setWeight(QFont.Weight.DemiBold if bold else QFont.Weight.Normal)
        for pixels in range(size, 17, -1):
            font.setPixelSize(pixels)
            metrics = QFontMetricsF(font)
            bounds = metrics.boundingRect(rect, flags, value)
            if bounds.width() <= w and bounds.height() <= h:
                break
        self.painter.setFont(font)
        self.painter.setPen(QColor(color or self.colors['text']))
        self.painter.drawText(rect, flags, value)

    def line(self, x, y, w):
        self.painter.setPen(QPen(QColor(self.colors['line']), 1))
        self.painter.drawLine(QPointF(x, y), QPointF(x + w, y))

    def icon(self, name, x, y, size=26, color=None):
        svg = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" '
               f'fill="none" stroke="{color or self.colors["text"]}" stroke-width="1.6" '
               f'stroke-linecap="round" stroke-linejoin="round">{ICON_PATHS[name]}</svg>')
        QSvgRenderer(QByteArray(svg.encode())).render(self.painter, QRectF(x, y, size, size))

    def hero(self):
        c = self.colors
        self.box(0, 0, 1200, 6, c['success'], radius=0)
        self.box(48, 40, 1104, 220, c['surface'], c['line'], radius=18)
        pieces = [(85, 74, 14, 5, -25, 'success'), (175, 96, 6, 12, 23, 'accent_text'),
                  (299, 70, 12, 5, 40, 'celebration_gold'), (416, 102, 5, 11, -28, 'success'),
                  (557, 72, 13, 5, -18, 'accent_text'), (710, 101, 6, 12, 28, 'celebration_gold'),
                  (856, 70, 12, 5, 35, 'success'), (940, 221, 12, 5, -30, 'accent_text'),
                  (1115, 75, 6, 13, 26, 'celebration_gold'), (1120, 218, 12, 5, -35, 'success')]
        for x, y, w, h, angle, token in pieces:
            self.painter.save()
            self.painter.translate(x, y)
            self.painter.rotate(angle)
            self.box(0, 0, w, h, c[token], radius=1)
            self.painter.restore()
        for x, y, size, token in [(916, 121, 8, 'celebration_gold'), (1099, 174, 7, 'accent_text'), (1012, 69, 6, 'success')]:
            self.painter.setPen(QPen(QColor(c[token]), 2))
            self.painter.drawLine(QPointF(x - size, y), QPointF(x + size, y))
            self.painter.drawLine(QPointF(x, y - size), QPointF(x, y + size))
        self.text(80, 122, 850, 80, self.t('Сита получена') + '!', 64, bold=True)
        self.text(83, 202, 850, 36, self.t('Поиск завершён'), 25, c['muted'])
        self.icon('calendar', 978, 90, 120, c['success'])

    def tile(self, x, value, title, icon):
        self.box(x, 292, 352, 142, self.colors['surface'], self.colors['line'])
        self.text(x + 24, 307, 304, 68, value, 45, bold=True)
        self.icon(icon, x + 24, 388, 24, self.colors['muted'])
        self.text(x + 60, 379, 274, 40, self.t(title), 22, self.colors['muted'], wrap=True)

    def group(self, title, icon, x, items):
        self.icon(icon, x, 480, 28)
        self.text(x + 42, 476, 484, 36, self.t(title), 25, bold=True)
        self.line(x, 528, 526)
        for index, (glyph, label, value) in enumerate(items):
            y = 538 + index * 54
            font = QFont('IBM Plex Sans')
            font.setPixelSize(24)
            font.setWeight(QFont.Weight.DemiBold)
            value_width = max(80, min(190, QFontMetricsF(font).horizontalAdvance(value) + 6))
            self.icon(glyph, x, y + 13, 24, self.colors['muted'])
            self.text(x + 38, y, 526 - 54 - value_width, 52, self.t(label), 22, wrap=True)
            self.text(x + 526 - value_width, y, value_width, 52, value, 24, bold=True, right=True)
            if index < len(items) - 1:
                self.line(x, y + 51, 526)


def render_achievement_card(achievement, language='en', dark=True):
    """Render only allowlisted statistics frozen when the user confirmed booking."""
    app = QApplication.instance()
    if app is not None:
        load_fonts(app)
    stats = snapshot(achievement.get('statistics', {}))
    card = _Card(language, dark)
    c = card.colors

    def value(key):
        item = stats.get(key)
        return '—' if item is None else str(item)

    def duration(key):
        from .achievement_exports import _duration
        seconds = stats.get(key)
        if seconds is None:
            return '—'
        if seconds < 60:
            return f'{int(seconds)} {card.t("с")}'
        return _duration(seconds, language)

    try:
        card.hero()
        card.tile(48, value('checks'), 'Проверок сит', 'search')
        card.tile(424, duration('active_seconds'), 'Активный поиск', 'clock')
        card.tile(800, value('days'), 'Дни с поиском', 'calendar_days')
        card.group('Доступность', 'eye', 48, [
            ('office', 'Появлений офисов', value('offices_seen')),
            ('pin', 'Разных офисов', value('unique_offices')),
            ('calendar_first', 'Появления выбора даты', value('booking_screens')),
            ('clipboard', '«Сит нет» до запроса', value('banners')),
            ('message', 'Ответы «Сит нет» на запрос записи', value('no_slots'))])
        card.group('Ошибки и восстановление', 'wrench', 626, [
            ('server', 'Ошибок сервера', value('server_errors')),
            ('hourglass', 'Ограничений HTTP 429', value('rate_limited')),
            ('lock', 'Ограничения доступа', value('blocked')),
            ('restart', 'Повторные проходы', value('flow_passes')),
            ('shield-alert', 'Ответы «Сит нет» с cod. oper', value('cod_oper_count'))])
        card.icon('settings', 48, 838, 28)
        card.text(90, 831, 1062, 40, card.t('Автоматизация'), 25, bold=True)
        card.line(48, 881, 1104)
        for x, key, label, icon in [(48, 'actions_performed', 'Действий выполнено', 'pointer'), (626, 'pages_processed', 'Страниц обработано', 'file_text')]:
            card.icon(icon, x, 908, 26, c['muted'])
            card.text(x + 40, 896, 486, 48, value(key), 32, bold=True)
            card.text(x, 946, 526, 36, card.t(label), 22, c['muted'])
        card.line(48, 1003, 1104)
        for x, prefix, icon, label in [(48, 'first_office', 'clock', 'Первый офис'), (626, 'first_booking', 'calendar', 'Первый выбор даты')]:
            amount = duration(prefix + '_seconds')
            if amount != '—':
                if stats[prefix + '_active_quality'] == 'minimum':
                    amount = '≥ ' + amount
                amount = card.t('{duration} поиска').format(duration=amount)
            card.icon(icon, x, 1031, 26, c['muted'])
            card.text(x + 40, 1017, 486, 40, card.t(label), 22, c['muted'])
            card.text(x + 40, 1060, 486, 40, amount, 27, bold=True)
        card.line(48, 1144, 1104)
        card.icon('activity', 48, 1172, 28, c['accent_text'])
        card.text(92, 1164, 1000, 42, 'Cita Extranjeria Helper', 25, bold=True)
        card.text(92, 1203, 1000, 28, 'github.com/itwormz/extranjeria-cita-helper', 19, c['muted'])
    finally:
        card.painter.end()
    return card.image
