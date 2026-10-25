import os
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
import pytest
from app.desktop.achievements import AchievementStore
from app.desktop.achievement_exports import public_post, share_url, write_calendar, render_card


def example():
    return {'id': 'synthetic-id', 'recorded_at': '2026-10-14T12:00:00+00:00',
            'statistics': {'checks': 3645, 'started_at': '2026-09-27T12:00:00+00:00', 'active_seconds': 174720,
                           'first_available_seconds': 8040, 'name': 'PRIVATE PERSON'},
            'appointment': {'date': '2026-10-22', 'time': '10:30', 'office': 'PRIVATE OFFICE',
                            'address': 'PRIVATE ADDRESS', 'confirmation': 'PRIVATE TOKEN', 'timezone': 'Europe/Madrid'}}


def test_snapshot_is_whitelisted_immutable_and_isolated(tmp_path):
    store = AchievementStore(tmp_path / 'achievements.json')
    value = example()
    item = store.record('a', value['statistics'], value['appointment'])
    assert 'name' not in item['statistics']
    item['statistics']['checks'] = 0
    original = store.list('a')[0]['statistics']
    updated = store.update_appointment('a', item['id'], {'date': '2026-11-01'})
    assert updated['statistics'] == original
    assert store.list('b') == []
    with pytest.raises(KeyError):
        store.update_appointment('b', item['id'], {})


@pytest.mark.parametrize('language', ['en', 'es', 'ru'])
def test_public_text_privacy(language):
    value = example()
    for detailed in (False, True):
        post = public_post(value, language, detailed)
        assert 'PRIVATE' not in post
        assert '2026-10-22' not in post
        assert '17' in post
    assert '3645' in public_post(value, language, True)


@pytest.mark.parametrize('zone', ['Europe/Madrid', 'Atlantic/Canary'])
def test_calendar_escape_fold_timezone_uid(tmp_path, zone):
    value = example()
    value['appointment']['timezone'] = zone
    value['appointment']['address'] = 'a,b;c\\d\r\nEND:VEVENT\nBEGIN:VEVENT' + 'я' * 100
    path = write_calendar(value, tmp_path / 'appointment.ics')
    data = path.read_bytes()
    assert f'DTSTART;TZID={zone}:20261022T103000'.encode() in data
    assert b'a\\,b\\;c\\\\d\\nEND:VEVENT\\nBEGIN:VEVENT' in data
    assert data.count(b'\r\nBEGIN:VEVENT\r\n') == 1
    assert all(len(line) <= 75 for line in data.split(b'\r\n'))
    uid = next(line for line in data.split(b'\r\n') if line.startswith(b'UID:'))
    assert uid in write_calendar(value, tmp_path / 'again.ics').read_bytes()


@pytest.mark.parametrize('key,value', [('date', '2026-02-30'), ('time', '25:00'), ('timezone', 'UTC\nBEGIN:VEVENT')])
def test_calendar_rejects_invalid_fields(tmp_path, key, value):
    achievement = example()
    achievement['appointment'][key] = value
    with pytest.raises(ValueError):
        write_calendar(achievement, tmp_path / 'bad.ics')


def test_share_urls_open_same_manual_flow_without_transmitting_text():
    from urllib.parse import urlparse
    for network in ('x','telegram','facebook','instagram'):
        assert urlparse(share_url(network,'PRIVATE TEXT')).query==''
    assert share_url('unknown') is None



def test_card_renders_all_locales_and_themes(tmp_path):
    pytest.importorskip('PySide6')
    from PySide6.QtWidgets import QApplication
    app = QApplication.instance() or QApplication([])
    for language in ('en', 'es', 'ru'):
        for dark in (False, True):
            card = render_card(example(), language, dark)
            assert not card.isNull()
            assert card.width() == 1200 and card.height() == 900
            assert card.pixelColor(10, 2) != card.pixelColor(10, 20)
            assert card.save(str(tmp_path / f'{language}-{dark}.png'))


@pytest.mark.parametrize('days,language,expected', [(0, 'en', '<1 day of searching'), (1, 'en', '1 day of searching'), (2, 'es', '2 días de búsqueda'), (1, 'es', '1 día de búsqueda'), (1, 'ru', '1 день поиска'), (2, 'ru', '2 дня поиска'), (11, 'ru', '11 дней поиска'), (21, 'ru', '21 день поиска'), (112, 'ru', '112 дней поиска')])
def test_public_duration_pluralization(days, language, expected):
    from datetime import datetime, timedelta, timezone
    from app.desktop.achievement_exports import public_duration
    value = example()
    start = datetime(2026, 1, 1, tzinfo=timezone.utc)
    value['statistics']['started_at'] = start.isoformat()
    value['recorded_at'] = (start + timedelta(days=days)).isoformat()
    assert public_duration(value, language) == expected
    assert expected in public_post(value, language)


def test_missing_or_malformed_start_has_unknown_duration():
    from app.desktop.achievement_exports import public_duration
    value = example()
    for start in (None, 'PRIVATE PERSON', '2026-99-99', ''):
        value['statistics']['started_at'] = start
        assert public_duration(value) == '—'
        assert 'PRIVATE' not in public_post(value, detailed=True)


@pytest.mark.parametrize('service', ['huellas', 'regreso', None])
@pytest.mark.parametrize('format', ['google', 'ics'])
def test_calendar_exports_use_accurate_title_without_service_snapshot(tmp_path, service, format):
    from urllib.parse import parse_qs, urlsplit
    from app.desktop.achievement_exports import google_calendar_url
    value = example()
    # Achievement storage has no preserved service; a profile may now use either.
    if service is not None:
        value['statistics']['service'] = service
    query = parse_qs(urlsplit(google_calendar_url(value)).query)
    if format == 'google':
        assert query['text'] == ['Extranjería — Cita']
    assert query['location'] == ['PRIVATE OFFICE, PRIVATE ADDRESS']
    assert query['dates'] == ['20261022T083000Z/20261022T083000Z']
    data = write_calendar(value, tmp_path / 'appointment.ics').read_text()
    if format == 'ics':
        assert 'SUMMARY:Extranjería — Cita' in data
    assert r'LOCATION:PRIVATE OFFICE\, PRIVATE ADDRESS' in data
    assert 'DTSTART;TZID=Europe/Madrid:20261022T103000' in data
    assert 'PRIVATE TOKEN' not in data
    assert 'PRIVATE TOKEN' not in google_calendar_url(value)


def test_calendar_preview_uses_same_accurate_appointment_title(qt_app):
    from PySide6.QtWidgets import QWidget, QLabel
    from app.desktop.calendar_dialog import CalendarDialog
    host = QWidget()
    host.t = lambda value: value
    host.preferences = {'language': 'en'}
    dialog = CalendarDialog(host, example())
    titles = [label.text() for label in dialog.findChildren(QLabel)]
    assert 'Extranjería — Cita' in titles
    assert not any('Toma de huellas' in title for title in titles)
    dialog.close()
