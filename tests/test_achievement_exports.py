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
    for network in ('x','telegram','facebook','instagram','reddit'):
        assert urlparse(share_url(network,'PRIVATE TEXT')).query==''
    assert urlparse(share_url('reddit')).hostname=='www.reddit.com'
    assert share_url('unknown') is None



def test_card_renders_all_locales_and_themes(tmp_path):
    pytest.importorskip('PySide6')
    from PySide6.QtWidgets import QApplication
    app = QApplication.instance() or QApplication([])
    for language in ('en', 'es', 'ru'):
        for dark in (False, True):
            card = render_card(example(), language, dark)
            assert not card.isNull()
            assert card.width() == 1200 and card.height() == 1240
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


def test_effort_statistics_preserve_unknown_old_snapshots():
    from app.desktop.achievements import snapshot
    old=snapshot({'checks':42})
    assert old['pages_processed'] is None and old['actions_performed'] is None
    text=public_post({'statistics':{'pages_processed':100,'actions_performed':250}},'en',True)
    assert '100' in text and '250' in text


@pytest.mark.parametrize('key', ['days','banners','no_slots','cod_oper_count','first_office_seconds','first_booking_seconds'])
def test_current_achievement_metrics_preserve_known_and_unknown_values(key):
    from app.desktop.achievements import snapshot
    assert snapshot({}).get(key) is None
    assert key in snapshot({})
    data={key:7,'first_office_active_quality':'exact','first_booking_active_quality':'minimum'}
    assert snapshot(data)[key]==7
    data[key]=0
    assert snapshot(data)[key]==0
    data[key]=float('nan')
    assert snapshot(data)[key] is None


def test_snapshot_keeps_first_event_quality_and_legacy_uncertainty(tmp_path):
    store=AchievementStore(tmp_path/'achievements.json')
    stats=dict(banners=22,no_slots=5,cod_oper_count=2,days=2,
               first_office_seconds=900,first_office_active_quality='minimum',
               first_booking_seconds=1200,first_booking_active_quality='unknown',
               legacy_unknown_fields=['banners'],name='PRIVATE PERSON')
    record=store.record('demo',stats,{})
    saved=store.list('demo')[0]['statistics']
    assert saved['banners'] is None
    assert saved['no_slots']==5 and saved['cod_oper_count']==2 and saved['days']==2
    assert saved['first_office_active_quality']=='minimum'
    assert saved['first_office_seconds']==900
    assert saved['first_booking_seconds'] is None
    stats['no_slots']=999
    assert store.list('demo')[0]['statistics']==saved
    assert 'PRIVATE' not in str(saved)


def test_public_card_does_not_depend_on_private_appointment_or_profile_data(qt_app):
    import copy
    original=example();other=copy.deepcopy(original)
    other['appointment']={key:'OTHER PRIVATE VALUE' for key in original['appointment']}
    other['statistics']['name']='OTHER PRIVATE PERSON'
    assert render_card(original,'ru',True)==render_card(other,'ru',True)


@pytest.mark.parametrize('language',['ru','en','es'])
@pytest.mark.parametrize('dark',[True,False])
def test_card_text_fits_with_current_metrics_and_large_counts(qt_app,monkeypatch,language,dark):
    from PySide6.QtCore import QRectF,Qt
    from PySide6.QtGui import QFontMetricsF
    from app.desktop import achievement_card
    drawn=[]
    original=achievement_card._Card.text
    def record(self,x,y,w,h,value,size=24,color=None,bold=False,right=False,wrap=False):
        original(self,x,y,w,h,value,size,color,bold,right,wrap)
        flags=(Qt.AlignmentFlag.AlignRight if right else Qt.AlignmentFlag.AlignLeft)|Qt.AlignmentFlag.AlignVCenter
        if wrap:flags|=Qt.TextFlag.TextWordWrap
        bounds=QFontMetricsF(self.painter.font()).boundingRect(QRectF(x,y,w,h),flags,value)
        assert bounds.width()<=w+0.01,(value,bounds.width(),w)
        assert bounds.height()<=h+0.01,(value,bounds.height(),h)
        assert x>=0 and y>=0 and x+w<=1200 and y+h<=1240
        drawn.append(value)
    monkeypatch.setattr(achievement_card._Card,'text',record)
    data=example()
    data['statistics'].update(days=2,banners=121,no_slots=131,cod_oper_count=141,
        checks=1234567890,flow_passes=1234567890,first_office_seconds=301,
        first_office_active_quality='minimum',first_booking_seconds=901,
        first_booking_active_quality='exact')
    render_card(data,language,dark)
    assert all(value in drawn for value in ['121','131','141','1234567890'])
    assert any(value.startswith('≥ ') for value in drawn)
    assert not any('PRIVATE' in value for value in drawn)
    drawn.clear()
    render_card(example(),language,dark)
    assert drawn.count('—')>=6


@pytest.mark.parametrize('language',['ru','en','es'])
def test_detailed_post_includes_full_current_snapshot(language):
    from app.desktop.i18n import translate
    data=example()
    counts={'checks':101,'days':102,'offices_seen':103,'unique_offices':104,
            'booking_screens':105,'banners':106,'no_slots':107,'cod_oper_count':108,
            'server_errors':109,'rate_limited':110,'blocked':111,'flow_passes':112,
            'actions_performed':113,'pages_processed':114}
    data['statistics'].update(counts,first_office_seconds=61,first_office_active_quality='minimum',
        first_booking_seconds=121,first_booking_active_quality='exact')
    text=public_post(data,language,True)
    for number in counts.values():assert str(number) in text
    assert translate('Первый офис',language) in text
    assert translate('Первый выбор даты',language) in text
    assert '≥ ' in text
    assert 'PRIVATE' not in text


@pytest.mark.parametrize('language',['ru','en','es'])
def test_detailed_post_keeps_unknown_new_counters_and_durations(language):
    from app.desktop.i18n import translate
    text=public_post(example(),language,True)
    for label in ['Дни с поиском','«Сит нет» до запроса','Ответы «Сит нет» на запрос записи',
                  'Ответы «Сит нет» с cod. oper','Первый офис','Первый выбор даты']:
        assert translate(label,language)+': —' in text
