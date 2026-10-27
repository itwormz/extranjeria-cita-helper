"""Offline public statistics cards and private appointment calendar exports."""
from datetime import datetime, timezone
from pathlib import Path
import uuid
from .achievements import snapshot

# Achievement records do not retain a service snapshot. Keep historical events accurate.
CALENDAR_TITLE = 'Extranjería — Cita'

_LABELS = {
'en': ('Sita obtained', 'days of searching', 'Checks', 'Active search', 'Office appearances', 'Unique offices', 'Booking screens', 'First availability', 'Server errors', 'HTTP 429 responses', 'Blocked responses', 'Passes from the start', 'Actions performed', 'Pages processed'),
'es': ('Cita conseguida', 'días de búsqueda', 'Comprobaciones', 'Búsqueda activa', 'Apariciones de oficinas', 'Oficinas distintas', 'Pantallas de cita', 'Primera disponibilidad', 'Errores del servidor', 'Respuestas HTTP 429', 'Bloqueos', 'Recorridos desde el inicio', 'Acciones realizadas', 'Páginas procesadas'),
'ru': ('Сита получена', 'дней поиска', 'Проверки', 'Активный поиск', 'Появления офисов', 'Разные офисы', 'Экраны записи', 'Первая доступность', 'Ошибки сервера', 'Ответы HTTP 429', 'Блокировки', 'Проходы с начала', 'Действий выполнено', 'Страниц обработано'),
}


def _duration(seconds,language='en'):
    if seconds is None:
        return '—'
    minutes = int(seconds) // 60
    hour_unit,minute_unit={'ru':('ч','мин'),'es':('h','min')}.get(language,('h','min'))
    if minutes>=60 and minutes%60==0:return f'{minutes//60} {hour_unit}'
    return f'{minutes//60} {hour_unit} {minutes%60} {minute_unit}' if minutes>=60 else f'{minutes} {minute_unit}'


def _public(achievement, language):
    labels = _LABELS.get(language, _LABELS['en'])
    stats = snapshot(achievement.get('statistics', {}))
    days = None
    try:
        start = datetime.fromisoformat(stats['started_at'])
        end = datetime.fromisoformat(achievement['recorded_at'])
        if start.tzinfo is None:
            start = start.replace(tzinfo=timezone.utc)
        if end.tzinfo is None:
            end = end.replace(tzinfo=timezone.utc)
        days = max(0, (end - start).days)
    except (ValueError, TypeError, KeyError):
        pass
    values = (stats['checks'], _duration(stats['active_seconds'],language), stats['offices_seen'], stats['unique_offices'],
              stats['booking_screens'], _duration(stats['first_available_seconds'],language), stats['server_errors'],
              stats['rate_limited'], stats['blocked'], stats['flow_passes'],stats['actions_performed'],stats['pages_processed'])
    return labels, days, tuple(zip(labels[2:], ('—' if value is None else str(value) for value in values)))


def public_duration(achievement, language='en'):
    """Localized elapsed search duration without inventing a start date."""
    _, days, _ = _public(achievement, language)
    if days is None:
        return '—'
    if days == 0:
        return {'en': '<1 day of searching', 'es': '<1 día de búsqueda', 'ru': '<1 дня поиска'}.get(language, '<1 day of searching')
    if language == 'ru':
        form = 'день' if days % 10 == 1 and days % 100 != 11 else 'дня' if days % 10 in (2, 3, 4) and days % 100 not in (12, 13, 14) else 'дней'
        return f'{days} {form} поиска'
    if language == 'es':
        return f"{days} {'día' if days == 1 else 'días'} de búsqueda"
    return f"{days} {'day' if days == 1 else 'days'} of searching"


def public_post(achievement, language='en', detailed=False):
    labels, days, rows = _public(achievement, language)
    lines = ['🎉 '+labels[0] + '!']
    search_duration=public_duration(achievement,language)
    if search_duration!='—':lines.append(search_duration+'.')
    if detailed:
        icons=('🔎','⏱','🏢','📍','📅','⌛','🔧','🚦','🛡','🔄','🖱','📄')
        lines.extend(f'{icon} {label}: {value}' for icon,(label,value) in zip(icons,rows))
    lines.append(_credit(language))
    return '\n'.join(lines)


def _credit(language):
    return {'ru':'Получено с помощью Cita Extranjeria Helper','en':'Obtained with Cita Extranjeria Helper','es':'Conseguido con Cita Extranjeria Helper'}.get(language,'Obtained with Cita Extranjeria Helper')


def share_url(network, text=''):
    return {'telegram':'https://web.telegram.org/','x':'https://x.com/','facebook':'https://www.facebook.com/','instagram':'https://www.instagram.com/'}.get(network)


def _ics_escape(value):
    return str(value).replace('\\', '\\\\').replace('\r\n', '\n').replace('\r', '\n').replace('\n', '\\n').replace(';', '\\;').replace(',', '\\,')


def _fold(line):
    # RFC 5545 limits physical lines to 75 UTF-8 octets, including continuation space.
    chunks, current = [], ''
    for char in line:
        if len((current + char).encode('utf-8')) > 75:
            chunks.append(current)
            current = ' ' + char
        else:
            current += char
    chunks.append(current)
    return '\r\n'.join(chunks)


def google_calendar_url(achievement):
    from urllib.parse import urlencode
    from zoneinfo import ZoneInfo
    appointment=achievement['appointment'];zone=appointment.get('timezone') or 'Europe/Madrid'
    if zone not in {'Europe/Madrid','Atlantic/Canary'}:raise ValueError('Unsupported appointment timezone')
    start=datetime.strptime(appointment['date']+' '+appointment['time'],'%Y-%m-%d %H:%M').replace(tzinfo=ZoneInfo(zone))
    instant=start.astimezone(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    location=', '.join(appointment.get(key,'') for key in ('office','address') if appointment.get(key))
    return 'https://calendar.google.com/calendar/render?'+urlencode(dict(action='TEMPLATE',text=CALENDAR_TITLE,dates=instant+'/'+instant,ctz=zone,location=location))


def write_calendar(achievement, path):
    appointment = achievement['appointment']
    zone = appointment.get('timezone') or 'Europe/Madrid'
    if zone not in {'Europe/Madrid', 'Atlantic/Canary'}:
        raise ValueError('Unsupported appointment timezone')
    date, time = appointment['date'], appointment['time']
    import re
    if not re.fullmatch(r'\d{4}-\d{2}-\d{2}', date) or not re.fullmatch(r'\d{2}:\d{2}', time):
        raise ValueError('Invalid appointment date or time')
    start = datetime.strptime(date + ' ' + time, '%Y-%m-%d %H:%M')
    identifier = uuid.uuid5(uuid.NAMESPACE_URL, 'cita-achievement:' + str(achievement['id']))
    location = ', '.join(appointment.get(k, '') for k in ('office', 'address') if appointment.get(k))
    summer_hour, winter_hour = ('020000', '030000') if zone == 'Europe/Madrid' else ('010000', '020000')
    standard, daylight = ('+0100', '+0200') if zone == 'Europe/Madrid' else ('+0000', '+0100')
    vtimezone = ['BEGIN:VTIMEZONE', f'TZID:{zone}', 'BEGIN:DAYLIGHT',
                 f'DTSTART:19700329T{summer_hour}', f'TZOFFSETFROM:{standard}', f'TZOFFSETTO:{daylight}',
                 'RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU', 'END:DAYLIGHT', 'BEGIN:STANDARD',
                 f'DTSTART:19701025T{winter_hour}', f'TZOFFSETFROM:{daylight}', f'TZOFFSETTO:{standard}',
                 'RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU', 'END:STANDARD', 'END:VTIMEZONE']
    lines = ['BEGIN:VCALENDAR', 'VERSION:2.0', 'PRODID:-//Cita Extranjeria Helper//Appointments//EN',
             'CALSCALE:GREGORIAN'] + vtimezone + ['BEGIN:VEVENT', f'UID:{identifier}@cita-helper.local',
             'DTSTAMP:' + datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ'),
             f'DTSTART;TZID={zone}:' + start.strftime('%Y%m%dT%H%M%S'),
             'SUMMARY:' + CALENDAR_TITLE, 'LOCATION:' + _ics_escape(location),
             'END:VEVENT', 'END:VCALENDAR']
    path = Path(path)
    path.write_bytes(('\r\n'.join(_fold(line) for line in lines) + '\r\n').encode('utf-8'))
    return path


def render_card(achievement, language='en', dark=True):
    from PySide6.QtCore import QRect, Qt
    from PySide6.QtGui import QColor, QFont, QImage, QPainter
    labels, days, rows = _public(achievement, language)
    image = QImage(1200, 900, QImage.Format.Format_ARGB32)
    from .theme import theme_colors
    colors=theme_colors(dark)
    image.fill(QColor(colors['surface']))
    painter = QPainter(image)
    painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    text = QColor(colors['text'])
    muted = QColor(colors['muted'])
    green = QColor(colors['success'])
    painter.fillRect(0, 0, 1200, 7, green)
    def draw(rect, value, size, color, bold=False):
        font = QFont('Arial', size)
        font.setBold(bold)
        from PySide6.QtGui import QFontMetrics
        while size > 10 and QFontMetrics(font).horizontalAdvance(value) > rect.width():
            size -= 1
            font.setPointSize(size)
        painter.setFont(font)
        painter.setPen(color)
        painter.drawText(rect, Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter, value)
    draw(QRect(64, 42, 1072, 80), '✓  ' + labels[0], 36, green, True)
    draw(QRect(64, 148, 1072, 75), public_duration(achievement, language), 27, text, True)
    for index, (label, value) in enumerate(rows):
        column, row = index % 2, index // 2
        x, y = 64 + column * 564, 270 + row * 82
        painter.setPen(QColor(colors['line']))
        painter.drawLine(x, y + 79, x + 508, y + 79)
        draw(QRect(x, y, 390, 74), label, 19, muted)
        draw(QRect(x + 390, y, 118, 74), value, 22, text, True)
    draw(QRect(64, 808, 1072, 45), _credit(language), 16, muted)
    painter.end()
    return image
