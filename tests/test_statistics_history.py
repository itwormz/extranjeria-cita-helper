import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
from datetime import date


def test_timeline_splits_midnight_and_uses_last_observation():
    from app.desktop.statistics_history import timeline_segments
    rows=[dict(id="same",kind="page",stage="cod_oper",mode="",start="2026-10-21T21:00:00+00:00",end=None,last_seen="2026-10-22T01:00:00+00:00",confirmed_end=False,event=False)]
    days=timeline_segments(rows,date(2026,10,21),3)
    assert len(days)==3
    assert [(x["left"],x["right"]) for x in days[0][1]]==[(23/24,1)]
    assert [(x["left"],x["right"]) for x in days[1][1]]==[(0,3/24)]
    assert not days[2][1]
    assert days[0][1][0]["row"]["id"]==days[1][1][0]["row"]["id"]


def test_timeline_does_not_extend_manual_or_gap_into_search():
    from app.desktop.statistics_history import segment_kind
    assert segment_kind(dict(kind="phase",mode="manual",stage=""))=="manual"
    assert segment_kind(dict(kind="phase",mode="unobserved",stage=""))=="gap"
    assert segment_kind(dict(kind="page",mode="",stage="cod_oper"))=="cod"


def test_office_count_popup_has_all_appearances_and_keyboard_access(qt_app):
    from app.desktop.statistics_history import OfficeCountButton
    rows=[dict(time=f"2026-10-{day:02d}T10:00:00+00:00",session_id="s") for day in range(1,8)]
    button=OfficeCountButton(rows,lambda x:x)
    button.show();button.setFocus();qt_app.processEvents();button.open_popup()
    assert button.popup.isVisible()
    assert len(button.preview_labels)==4
    button.show_all()
    assert button.full_list.count()==7
    button.popup.close();button.close()


def test_history_only_queries_visible_range_and_localizes_calendar(qt_app):
    from app.desktop.statistics_history import StatisticsHistory
    from types import SimpleNamespace
    from app.desktop.i18n import translate
    class Store:
        def __init__(self):self.calls=[]
        def history_bounds(self,*args,**kwargs):return {'start':'2026-10-01T10:00:00+00:00','end':'2026-10-23T12:00:00+00:00'}
        def history(self,*args,**kwargs):self.calls.append(kwargs);return []
        def office_history(self,*args,**kwargs):return []
    store=Store();host=SimpleNamespace(preferences={'language':'es','dark':False},t=lambda text:translate(text,'es'))
    panel=StatisticsHistory(host,store);panel.refresh('one','all');panel.refresh('one','all')
    assert all(call.get('start') and call.get('end') for call in store.calls)
    assert store.calls[-1]['start'].date().isoformat()=='2026-10-01'
    panel.open_period();calendar=panel.period_from.calendarWidget()
    assert calendar.objectName()=='appointmentCalendar'
    assert calendar.locale().name()=='es_ES'
    panel.period_from.setDate(__import__('PySide6.QtCore',fromlist=['QDate']).QDate(2026,10,18))
    panel.period_apply.click()
    assert store.calls[-1]['start'].date().isoformat()=='2026-10-18'
    assert store.calls[-1]['start'].astimezone(__import__('zoneinfo').ZoneInfo('Europe/Madrid')).hour==0
    panel.refresh('one','today')
    assert len(panel.timeline.days)==1
    panel.deleteLater()


def test_dst_local_hour_ticks_follow_actual_elapsed_day():
    from app.desktop.statistics_history import day_ticks
    from datetime import date
    spring=dict(day_ticks(date(2026,3,29)))
    autumn=dict(day_ticks(date(2026,10,25)))
    assert spring['06:00']==5/23
    assert autumn['06:00']==7/25
    assert spring['24:00']==autumn['24:00']==1


def test_details_use_readable_page_labels_and_exact_http_status(qt_app):
    from app.desktop.statistics_history import StatisticsHistory
    from app.desktop.i18n import translate
    from types import SimpleNamespace
    panel=StatisticsHistory(SimpleNamespace(preferences={'language':'en','dark':True},t=lambda text:translate(text,'en')),None)
    panel.show_details([dict(kind='page',mode='',stage='rate_limited',start='2026-10-23T10:00:00+00:00',end=None,last_seen='2026-10-23T11:00:00+00:00',confirmed_end=False,event=False,evidence={'codes':[],'http_status':429})])
    assert 'rate_limited' not in panel.detail.text()
    assert 'HTTP 429' in panel.detail.text()
    assert 'End unknown' in panel.detail.text()
    panel.deleteLater()


def test_office_count_russian_grammar_and_explicit_close(qt_app):
    from app.desktop.statistics_history import OfficeCountButton
    from PySide6.QtWidgets import QPushButton
    for count,expected in [(1,'Предложен 1 раз'),(2,'Предложен 2 раза'),(12,'Предложен 12 раз'),(21,'Предложен 21 раз')]:
        button=OfficeCountButton([dict(time='2026-10-23T10:00:00+00:00')]*count,lambda text:text)
        assert button.text()==expected
        button.show_all();qt_app.processEvents()
        close=next(child for child in button.full_dialog.findChildren(QPushButton) if child.text()=='Закрыть')
        close.click();assert not button.full_dialog.isVisible();button.deleteLater()


def test_today_history_follows_madrid_midnight(qt_app,monkeypatch):
    from datetime import datetime,timezone
    from types import SimpleNamespace
    from app.desktop import statistics_history as module
    class Clock(datetime):
        current=datetime(2026,1,1,22,59,tzinfo=timezone.utc)
        @classmethod
        def now(cls,tz=None):return cls.current.astimezone(tz)
    class Store:
        def history_bounds(self,*a,**k):return {'start':None,'end':None}
        def office_history(self,*a,**k):return []
        def history(self,*a,**k):self.last=k;return []
    monkeypatch.setattr(module,'datetime',Clock)
    store=Store();host=SimpleNamespace(t=lambda t:t,preferences={'dark':True,'language':'ru'})
    panel=module.StatisticsHistory(host,store);panel.refresh('a','today')
    Clock.current=datetime(2026,1,1,23,1,tzinfo=timezone.utc)
    panel.refresh('a','today')
    assert store.last['start'].astimezone(module.MADRID).date().isoformat()=='2026-01-02'
    panel.deleteLater()


def test_cod_color_requires_operation_code_evidence():
    from app.desktop.statistics_history import segment_kind
    row=dict(kind='page',stage='request_no_slots_restart',mode='',evidence=[])
    assert segment_kind(row)=='search'
    row['evidence']=['operation_code','no_appointments']
    assert segment_kind(row)=='cod'


def test_hidden_office_list_does_not_stretch_timeline(qt_app,tmp_path):
    from types import SimpleNamespace
    from app.desktop.statistics import ProfileStatistics
    from app.desktop.statistics_history import StatisticsHistory
    store=ProfileStatistics(tmp_path/'s.json')
    store.observe('a','office','d',offices=[f'Office {i}, Example street address {i}' for i in range(20)])
    panel=StatisticsHistory(SimpleNamespace(t=lambda t:t,preferences={'dark':True,'language':'ru'}),store)
    panel.resize(1000,500);panel.refresh('a','all');panel.select_tab(0)
    panel.show();qt_app.processEvents()
    assert panel.sizeHint().height()<600
    panel.deleteLater();panel.deleteLater()


def test_timeline_coalesces_backgrounds_without_hiding_cod():
    from app.desktop.statistics_history import timeline_segments
    def row(id,kind,stage,start,end):
        return dict(id=id,kind=kind,stage=stage,mode='',start=f'2026-10-23T{start}:00+00:00',end=f'2026-10-23T{end}:00+00:00',last_seen=f'2026-10-23T{end}:00+00:00',confirmed_end=True,event=False)
    rows=[row('phase','phase','','08:00','14:00'),row('a','page','cod_oper','09:00','10:00'),row('b','page','cod_oper','10:00','11:00')]
    bands=timeline_segments(rows,date(2026,10,23),1)[0][1]
    assert [s['kind'] for s in bands]==['search','cod','search']
    assert all(a['right']<=b['left'] for a,b in zip(bands,bands[1:]))
    assert bands[1]['left']==11/24 and bands[1]['right']==13/24


def test_history_default_selection_survives_refresh_and_controls_share_header(qt_app):
    from app.desktop.statistics_history import StatisticsHistory
    from types import SimpleNamespace
    row=dict(id='cod',kind='page',stage='cod_oper',mode='',start='2026-10-23T10:00:00+00:00',end=None,last_seen='2026-10-23T11:00:00+00:00',confirmed_end=False,event=False,evidence=[])
    class Store:
        def history_bounds(self,*a,**k):return {'end':row['last_seen']}
        def office_history(self,*a,**k):return []
        def history(self,*a,**k):return [dict(row)]
    panel=StatisticsHistory(SimpleNamespace(t=lambda t:t,preferences={'dark':True,'language':'ru'}),Store())
    panel.resize(1000,350);panel.refresh('a','all');panel.show();qt_app.processEvents()
    assert panel.detail.text()
    assert panel.period_button.mapTo(panel,panel.period_button.rect().center()).y()==panel.history_button.mapTo(panel,panel.history_button.rect().center()).y()
    assert panel.timeline.height()<=190
    panel.show_details([row]);before=panel.detail.text();panel.refresh('a','all')
    assert panel.detail.text()==before
    panel.deleteLater()


def test_history_header_fits_compact_window_with_theme(qt_app):
    from app.desktop.statistics_history import StatisticsHistory
    from app.desktop.theme import apply_theme
    from types import SimpleNamespace
    class Store:
        def history_bounds(self,*a,**k):return {'end':'2026-10-23T12:00:00+00:00'}
        def office_history(self,*a,**k):return []
        def history(self,*a,**k):return []
    apply_theme(qt_app,dark=True)
    panel=StatisticsHistory(SimpleNamespace(t=lambda t:t,preferences={'dark':True,'language':'ru'}),Store())
    panel.refresh('a','all');panel.resize(590,350);panel.show();qt_app.processEvents()
    assert panel.minimumSizeHint().width()<=590
    panel.deleteLater()


def test_booking_handoff_is_manual_and_booking_page_is_not_green():
    from app.desktop.statistics_history import segment_kind
    assert segment_kind(dict(kind='phase',mode='booking_handoff',stage='booking'))=='manual'
    assert segment_kind(dict(kind='page',mode='',stage='booking'))!='available'


def test_shared_axis_projects_dst_observations_to_local_clock():
    from app.desktop.statistics_history import day_ticks,wall_clock_fraction,wall_clock_ranges
    for day in (date(2026,3,29),date(2026,10,25)):
        assert wall_clock_fraction(day,dict(day_ticks(day))['06:00'])==6/24
        assert wall_clock_fraction(day,0)==0
        assert wall_clock_fraction(day,1)==1
    assert wall_clock_ranges(date(2026,3,29),0,1)==[(0,2/24),(3/24,1)]
    assert wall_clock_ranges(date(2026,10,25),0,1)==[(0,3/24),(2/24,1)]


def test_zero_duration_observation_is_retained_without_fake_duration():
    from app.desktop.statistics_history import timeline_segments
    row=dict(id='point',kind='page',stage='cod_oper',mode='',start='2026-10-23T10:00:00+00:00',end=None,last_seen='2026-10-23T10:00:00+00:00',confirmed_end=False,event=False)
    segments=timeline_segments([row],date(2026,10,23),1)[0][1]
    assert len(segments)==1
    assert segments[0]['left']==segments[0]['right']==0.5


def test_zero_duration_observation_at_midnight_belongs_to_new_day():
    from app.desktop.statistics_history import timeline_segments
    row=dict(id='midnight',kind='page',stage='cod_oper',mode='',start='2026-10-22T22:00:00+00:00',end=None,last_seen='2026-10-22T22:00:00+00:00',confirmed_end=False,event=False)
    days=timeline_segments([row],date(2026,10,22),2)
    assert not days[0][1]
    assert len(days[1][1])==1
    assert days[1][1][0]['left']==days[1][1][0]['right']==0


def test_generated_gap_detail_shows_range_without_claiming_observation(qt_app):
    from types import SimpleNamespace
    from app.desktop.statistics_history import StatisticsHistory
    from app.desktop.i18n import translate
    panel=StatisticsHistory(SimpleNamespace(t=lambda t:translate(t,'en'),preferences={'dark':True,'language':'en'}),None)
    panel.show_details([dict(kind='phase',mode='unobserved',stage='',start='2026-10-23T08:00:00+00:00',end='2026-10-23T10:00:00+00:00',last_seen='2026-10-23T08:00:00+00:00',confirmed_end=False,event=False)])
    assert '10:00:00' in panel.detail.text() and '12:00:00' in panel.detail.text()
    assert 'End unknown' not in panel.detail.text()
    assert 'Last observation' not in panel.detail.text()
    panel.deleteLater()


def test_details_order_rollback_instants_in_utc_and_disambiguate_offsets(qt_app):
    from types import SimpleNamespace
    from app.desktop.statistics_history import StatisticsHistory
    panel=StatisticsHistory(SimpleNamespace(t=lambda t:t,preferences={'dark':True,'language':'ru'}),None)
    def row(start,end):return dict(kind='page',stage='cod_oper',mode='',start=start,end=end,last_seen=end,confirmed_end=True,event=False)
    panel.show_details([row('2026-10-25T00:50:00+00:00','2026-10-25T00:55:00+00:00'),row('2026-10-25T01:05:00+00:00','2026-10-25T01:10:00+00:00')])
    assert '02:50:00 CEST' in panel.detail.text() and '02:10:00 CET' in panel.detail.text()
    assert '10 мин' in panel.detail.text()
    panel.deleteLater()


def test_selected_split_phase_uses_visual_boundaries_and_retains_them(qt_app):
    from types import SimpleNamespace
    from app.desktop.statistics_history import StatisticsHistory
    def row(id,kind,stage,start,end):return dict(id=id,kind=kind,stage=stage,mode='',start=f'2026-10-23T{start}:00+00:00',end=f'2026-10-23T{end}:00+00:00',last_seen=f'2026-10-23T{end}:00+00:00',confirmed_end=True,event=False)
    rows=[row('phase','phase','','08:00','14:00'),row('cod','page','cod_oper','09:00','11:00')]
    class Store:
        def history_bounds(self,*a,**k):return {'end':rows[0]['end']}
        def office_history(self,*a,**k):return []
        def history(self,*a,**k):return rows
    panel=StatisticsHistory(SimpleNamespace(t=lambda t:t,preferences={'dark':True,'language':'ru'}),Store())
    panel.resize(1000,350);panel.refresh('a','all');panel.show();qt_app.processEvents()
    selected=next(sources for rect,sources in panel.timeline.hit if sources[0].get('id')=='phase')
    panel.show_details(selected)
    assert '10:00:00' in panel.detail.text() and '11:00:00' in panel.detail.text()
    assert '1 ч 0 мин' in panel.detail.text()
    panel.refresh('a','all')
    assert '10:00:00' in panel.detail.text() and '11:00:00' in panel.detail.text()
    assert '1 ч 0 мин' in panel.detail.text()
    assert rows[0]['start']=='2026-10-23T08:00:00+00:00'
    assert not any(key.startswith('_display') for source in rows for key in source)
    panel.deleteLater()


def test_cod_detail_uses_cod_legend_label_for_restart_page(qt_app):
    from types import SimpleNamespace
    from app.desktop.statistics_history import StatisticsHistory
    panel=StatisticsHistory(SimpleNamespace(t=lambda t:t,preferences={'dark':True,'language':'ru'}),None)
    panel.show_details([dict(kind='page',stage='request_no_slots_restart',mode='',start='2026-10-23T10:00:00+00:00',end=None,last_seen='2026-10-23T11:00:00+00:00',confirmed_end=False,event=False,evidence=['operation_code'])])
    assert 'Нет сит · Cod. Oper.' in panel.detail.text()
    assert 'новый проход' not in panel.detail.text()
    panel.deleteLater()


def test_all_observed_days_and_custom_inclusive_period(qt_app):
    from app.desktop.statistics_history import StatisticsHistory
    from types import SimpleNamespace
    from PySide6.QtCore import QDate
    rows=[dict(id=i,kind="page",stage="unknown",mode="",start=f"2025-12-{day}T10:00:00Z",end=None,last_seen=f"2025-12-{day}T10:00:00Z",event=False,confirmed_end=False) for i,day in enumerate((29,31))]
    class Store:
        def history_bounds(self,*a,**k):return dict(start=rows[0]["start"],end=rows[-1]["start"])
        def history(self,*a,**k):return rows
        def office_history(self,*a,**k):return []
    panel=StatisticsHistory(SimpleNamespace(preferences=dict(language="en",dark=True),t=lambda x:x),Store())
    panel.refresh("one","all")
    assert [day.day for day,_ in panel.timeline.days]==[29,31]
    panel.open_period();panel.period_from.setDate(QDate(2025,12,31));panel.period_to.setDate(QDate(2025,12,29))
    assert not panel.period_apply.isEnabled()
    panel.period_from.setDate(QDate(2025,12,29));panel.period_to.setDate(QDate(2026,1,1));panel.period_apply.click()
    assert len(panel.timeline.days)==4
    panel.refresh("two","all");assert panel.custom_range is None
    panel.refresh("two","today");assert not panel.period_button.isEnabled();assert len(panel.timeline.days)==1
    panel.deleteLater()


def test_timeline_axis_is_three_hourly_and_zoom_preserves_actual_duration(qt_app):
    from types import SimpleNamespace
    from app.desktop.statistics_history import Timeline
    host=SimpleNamespace(t=lambda t:t,preferences={'dark':True,'language':'ru'})
    timeline=Timeline(host);timeline.resize(1100,100)
    row=dict(id='cod',kind='page',stage='cod_oper',mode='',start='2026-10-23T10:00:00Z',end='2026-10-23T10:03:10Z',last_seen='2026-10-23T10:03:10Z',confirmed_end=True,event=False)
    timeline.set_rows([row],date(2026,10,23),1);timeline.show();qt_app.processEvents()
    labels=[tick for tick in timeline.axis_ticks() if tick[1]]
    assert [second for second,_ in labels]==list(range(0,86401,10800))
    assert len(timeline.axis_ticks())==9
    old=next(rect.width() for rect,rows in timeline.hit if rows[0].get('id')=='cod')
    timeline.zoom(0.5,anchor=0.5);qt_app.processEvents()
    new=next(rect.width() for rect,rows in timeline.hit if rows[0].get('id')=='cod')
    assert abs(new-2*old)<0.01
    assert row['start']=='2026-10-23T10:00:00Z'
    assert abs(timeline.clock_at(72+(timeline.width()-82)/2)-12*3600)<1
    timeline.deleteLater()


def test_timeline_tooltip_contains_clock_and_observed_cod_duration(qt_app):
    from types import SimpleNamespace
    from PySide6.QtCore import QPointF,QEvent,Qt
    from PySide6.QtGui import QMouseEvent
    from PySide6.QtWidgets import QToolTip
    from app.desktop.statistics_history import Timeline
    timeline=Timeline(SimpleNamespace(t=lambda t:t,preferences={'dark':True,'language':'ru'}));timeline.resize(1100,100)
    row=dict(id='cod',kind='page',stage='cod_oper',mode='',start='2026-10-23T10:00:00Z',end='2026-10-23T10:03:10Z',last_seen='2026-10-23T10:03:10Z',confirmed_end=True,event=False)
    timeline.set_rows([row],date(2026,10,23),1);timeline.show();qt_app.processEvents()
    rect=next(rect for rect,rows in timeline.hit if rows[0].get('id')=='cod')
    point=rect.center();event=QMouseEvent(QEvent.Type.MouseMove,point,QPointF(timeline.mapToGlobal(point.toPoint())),Qt.MouseButton.NoButton,Qt.MouseButton.NoButton,Qt.KeyboardModifier.NoModifier)
    qt_app.sendEvent(timeline,event);qt_app.processEvents()
    from PySide6.QtTest import QTest
    QTest.qWait(400)
    assert 'Cod. Oper.' in QToolTip.text()
    assert '3 мин 10 с' in QToolTip.text()
    assert '23.10.2026' in QToolTip.text()
    QToolTip.hideText();timeline.deleteLater()


def test_office_marker_differs_from_booking_marker(qt_app):
    from types import SimpleNamespace
    from app.desktop.statistics_history import Timeline
    from app.desktop.theme import theme_colors
    timeline=Timeline(SimpleNamespace(t=lambda t:t,preferences={'dark':True,'language':'ru'}));timeline.resize(1100,100)
    def row(kind,hour):return dict(id=kind,kind=kind,stage=kind,mode='',start=f'2026-10-23T{hour}:00:00Z',end=f'2026-10-23T{hour}:00:00Z',last_seen=f'2026-10-23T{hour}:00:00Z',confirmed_end=True,event=True)
    timeline.set_rows([row('office','08'),row('booking','12')],date(2026,10,23),1);timeline.show();qt_app.processEvents()
    office=next(rect for rect,rows in timeline.hit if rows[0].get('kind')=='office');booking=next(rect for rect,rows in timeline.hit if rows[0].get('kind')=='booking')
    assert office.height()==booking.height()==18  # Equal accessible targets; different painted symbols.
    image=timeline.grab().toImage();colors=theme_colors(True)
    assert any(image.pixelColor(x,y).name().upper()==colors['history_office'].upper() for x in range(int(office.left()),int(office.right())) for y in range(int(office.top()),int(office.bottom())))
    assert colors['history_office']!=colors['history_available']
    timeline.deleteLater()


def test_dense_observations_stay_inside_flat_lane(qt_app):
    from types import SimpleNamespace
    from app.desktop.statistics_history import Timeline
    timeline=Timeline(SimpleNamespace(t=lambda s:s,preferences={'dark':True}))
    row=dict(id='isolated',kind='page',stage='server_error',mode='',start='2026-10-23T10:00:00Z',end=None,last_seen='2026-10-23T10:00:00Z',confirmed_end=False,event=False)
    timeline.resize(1000,100);timeline.set_rows([row],date(2026,10,23),1);timeline.show();qt_app.processEvents()
    rect=next(rect for rect,rows in timeline.hit if rows[0].get('id')=='isolated')
    assert rect.height()==10
    assert timeline.height()<=84
    timeline.deleteLater()


def test_close_office_events_group_and_separate_when_zooming(qt_app):
    from types import SimpleNamespace
    from app.desktop.statistics_history import Timeline
    timeline=Timeline(SimpleNamespace(t=lambda s:s,preferences={'dark':True}));timeline.resize(1000,100)
    rows=[dict(id=n,kind='office',stage='office',mode='',start=f'2026-10-23T10:{n:02}:00Z',end=None,last_seen=f'2026-10-23T10:{n:02}:00Z',confirmed_end=True,event=True,offices=[f'Office {n}']) for n in (0,5)]
    timeline.set_rows(rows,date(2026,10,23),1);timeline.show();qt_app.processEvents()
    markers=[sources for rect,sources in timeline.hit if sources[0]['kind']=='office']
    assert len(markers)==1 and len(markers[0])==2
    assert all(f'Office {n}' in timeline.hover_text(next(rect.center() for rect,ss in timeline.hit if ss[0]['kind']=='office')) for n in (0,5))
    timeline.zoom(1/48,anchor=12/24);qt_app.processEvents()
    markers=[sources for rect,sources in timeline.hit if sources[0]['kind']=='office']
    assert len(markers)==2
    timeline.deleteLater()


def test_compact_legend_opens_complete_explanation(qt_app):
    from app.desktop.statistics_history import StatisticsHistory
    from types import SimpleNamespace
    from PySide6.QtWidgets import QLabel
    class Store:
        def history_bounds(self,*a,**k):return None
        def history(self,*a,**k):return []
        def office_history(self,*a,**k):return []
    panel=StatisticsHistory(SimpleNamespace(t=lambda s:s,preferences={'dark':True,'language':'ru'}),Store());panel.resize(640,500);panel.refresh('p','all');panel.show();qt_app.processEvents()
    assert 'Ручное управление' not in panel.legend.text()
    panel.legend_button.click();qt_app.processEvents()
    assert panel.legend_popup.isVisible()
    text=' '.join(label.text() for label in panel.legend_popup.findChildren(QLabel))
    assert 'Ручное управление' in text and 'Новая сессия' in text and 'HTTP 429' in text
    panel.legend_popup.close();panel.deleteLater()
