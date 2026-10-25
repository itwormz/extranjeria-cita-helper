from pathlib import Path
from types import SimpleNamespace
from app.desktop.statistics import ProfileStatistics
from app.desktop.statistics_history import segment_kind, Timeline


def test_live_wait_keeps_error_until_next_page(tmp_path):
    store=ProfileStatistics(tmp_path/'s.json')
    store.start_session('p','s',{},at='2026-01-01T10:00:00Z')
    store.observe_page('p','s','server_error','bad',at='2026-01-01T10:00:00Z')
    for minute,second in [(0,30),(1,0),(1,30)]:
        at=f'2026-01-01T10:0{minute}:{second:02}Z'
        store.phase('p','s','waiting',at=at)
        store.heartbeat('p','s',at=at)
    store.observe_page('p','s','province','good',at='2026-01-01T10:02:00Z')
    row=next(r for r in store.history('p') if r['stage']=='server_error')
    assert row['end']=='2026-01-01T10:02:00+00:00'
    assert row['confirmed_end']


def test_explicit_gap_does_not_bridge_error(tmp_path):
    store=ProfileStatistics(tmp_path/'s.json')
    store.observe_page('p','s','server_error','bad',at='2026-01-01T10:00:00Z')
    store.phase('p','s','unobserved',at='2026-01-01T10:01:00Z')
    store.observe_page('p','s','province','good',at='2026-01-01T10:02:00Z')
    row=next(r for r in store.history('p') if r['stage']=='server_error')
    assert row['end']=='2026-01-01T10:00:00+00:00'
    assert not row['confirmed_end']


def test_timeline_separates_session_anomalies_and_site_failures():
    for stage in ('expired','blocked'):
        assert segment_kind(dict(kind='page',stage=stage,mode=''))=='anomaly'
    for stage in ('server_error','rate_limited','unknown'):
        assert segment_kind(dict(kind='page',stage=stage,mode=''))=='recovery'
    for stage in ('request_no_slots','post_contact_no_slots','request_menu'):
        assert segment_kind(dict(kind='page',stage=stage,mode=''))=='search'
    for stage in ('request_no_slots','request_no_slots_restart','post_contact_no_slots'):
        assert segment_kind(dict(kind='page',stage=stage,mode='',evidence=['operation_code']))=='cod'
    assert segment_kind(dict(kind='browser_session_started',stage='',mode=''))=='restart'


def test_zoom_preserves_pointer_time(qt_app):
    timeline=Timeline(SimpleNamespace(t=lambda s:s,preferences={'dark':True}))
    timeline.zoom(.5,anchor=.25)
    assert abs(timeline.view_start-.125)<1e-9
    assert abs(timeline.view_end-.625)<1e-9
    timeline.deleteLater()


def test_hover_only_contains_data_and_office_names(qt_app):
    from PySide6.QtCore import QRectF,QPointF
    timeline=Timeline(SimpleNamespace(t=lambda s:s,preferences={'dark':True}))
    row=dict(kind='office',stage='office',mode='',event=True,start='2026-01-01T10:00:00Z',offices=['Office A','Office B'])
    timeline.hit=[(QRectF(80,40,40,20),[row])]
    text=timeline.hover_text(QPointF(90,50))
    assert 'Office A' in text and 'Office B' in text
    assert 'Europe/Madrid' not in text and 'Ctrl' not in text
    timeline.deleteLater()


def test_trackpad_pan_vertical_scroll_and_pinch(qt_app):
    from PySide6.QtCore import QPoint,QPointF,Qt,QEvent
    from PySide6.QtGui import QWheelEvent,QNativeGestureEvent,QPointingDevice
    timeline=Timeline(SimpleNamespace(t=lambda s:s,preferences={'dark':True}))
    timeline.resize(1000,100);timeline.zoom(.5,anchor=.5)
    def wheel(x,y):
        return QWheelEvent(QPointF(500,50),QPointF(500,50),QPoint(x,y),QPoint(),Qt.MouseButton.NoButton,Qt.KeyboardModifier.NoModifier,Qt.ScrollPhase.ScrollUpdate,False)
    vertical=wheel(0,40);timeline.wheelEvent(vertical)
    assert not vertical.isAccepted()
    before=timeline.view_start
    horizontal=wheel(40,0);timeline.wheelEvent(horizontal)
    assert horizontal.isAccepted() and timeline.view_start<before
    span=timeline.view_end-timeline.view_start
    gesture=QNativeGestureEvent(Qt.NativeGestureType.ZoomNativeGesture,QPointingDevice.primaryPointingDevice(),2,QPointF(500,50),QPointF(500,50),QPointF(500,50),.1,QPointF())
    assert timeline.event(gesture)
    assert timeline.view_end-timeline.view_start<span
    timeline.deleteLater()


def test_browser_generations_are_not_flow_restarts(tmp_path):
    from app.assist_control import AssistControl
    from app.desktop.statistics_session import StatisticsSession
    stats=ProfileStatistics(tmp_path/'s.json');control=AssistControl()
    task=SimpleNamespace(id='p',province='Barcelona',procedure='huellas',access_mode='certificate')
    session=StatisticsSession(stats,task,control)
    control.browser_session_started()
    control.observe_visit(document='blocked',stage='blocked',page='',title='')
    control.record('flow_restart',stage='province')
    control.browser_session_started()
    rows=[r for r in stats.history('p') if r['kind']=='browser_session_started']
    assert [r['evidence']['browser_generation'] for r in rows]==[1,2]
    assert rows[-1]['evidence']['previous_stage']=='blocked'
    session.close('stopped')


def test_transient_loading_does_not_turn_anomaly_into_normal_search(tmp_path):
    stats=ProfileStatistics(tmp_path/'s.json')
    stats.observe_page('p','s','expired','e',at='2026-01-01T10:00:00Z')
    stats.observe_page('p','s','loading','l',at='2026-01-01T10:00:10Z')
    stats.observe_page('p','s','province','p',at='2026-01-01T10:00:20Z')
    rows=[r for r in stats.history('p') if r['kind']=='page']
    assert [r['stage'] for r in rows]==['expired','province']
    assert rows[0]['end']=='2026-01-01T10:00:20+00:00'


def test_contiguous_same_state_ignores_documents_but_not_stage_changes():
    from datetime import date
    from app.desktop.statistics_history import timeline_segments
    def page(n,stage,start,end):
        return dict(id=n,kind='page',stage=stage,mode='',session_id='s',document=str(n),start=f'2026-01-01T10:{start:02}:00Z',end=f'2026-01-01T10:{end:02}:00Z',last_seen=f'2026-01-01T10:{end:02}:00Z',confirmed_end=True,event=False)
    segments=timeline_segments([page(1,'request_no_slots',0,5),page(2,'request_no_slots',5,10),page(3,'province',10,15)],date(2026,1,1),1)[0][1]
    assert len(segments)==2
    assert len(segments[0]['rows'])==2
    assert segments[1]['row']['stage']=='province'


def test_background_drag_pans_without_selecting_and_click_selects(qt_app):
    from PySide6.QtCore import QEvent,QPointF,Qt
    from PySide6.QtGui import QMouseEvent
    timeline=Timeline(SimpleNamespace(t=lambda s:s,preferences={'dark':True}));timeline.resize(1000,100)
    timeline.zoom(.5,anchor=.5)
    selected=[];timeline.selected.connect(selected.append)
    from PySide6.QtCore import QRectF
    timeline.hit=[(QRectF(400,45,120,10),[dict(id=1)])]
    def send(kind,x,button,buttons):
        event=QMouseEvent(kind,QPointF(x,50),QPointF(x,50),button,buttons,Qt.KeyboardModifier.NoModifier)
        qt_app.sendEvent(timeline,event)
    before=timeline.view_start
    send(QEvent.Type.MouseButtonPress,600,Qt.MouseButton.LeftButton,Qt.MouseButton.LeftButton)
    send(QEvent.Type.MouseMove,700,Qt.MouseButton.NoButton,Qt.MouseButton.LeftButton)
    send(QEvent.Type.MouseButtonRelease,700,Qt.MouseButton.LeftButton,Qt.MouseButton.NoButton)
    assert timeline.view_start<before and not selected
    send(QEvent.Type.MouseButtonPress,450,Qt.MouseButton.LeftButton,Qt.MouseButton.LeftButton)
    send(QEvent.Type.MouseButtonRelease,450,Qt.MouseButton.LeftButton,Qt.MouseButton.NoButton)
    assert len(selected)==1
    timeline.deleteLater()


def test_selected_state_has_full_visible_range_outline(qt_app):
    from datetime import date
    from app.desktop.statistics_history import Timeline
    timeline=Timeline(SimpleNamespace(t=lambda s:s,preferences={'dark':True}));timeline.resize(1000,100)
    def row(n,start,end):return dict(id=n,kind='page',stage='cod_oper',mode='',session_id='s',start=f'2026-01-01T10:{start:02}:00Z',end=f'2026-01-01T10:{end:02}:00Z',last_seen=f'2026-01-01T10:{end:02}:00Z',confirmed_end=True,event=False)
    rows=[row(1,0,10),row(2,10,20)]
    timeline.set_rows(rows,date(2026,1,1),1);timeline.show();qt_app.processEvents()
    rect,sources=next((rect,sources) for rect,sources in timeline.hit if sources[0].get('stage')=='cod_oper')
    assert len(sources)==2
    timeline.selected_key=(timeline.row_key(sources[0]),sources[0]['_display_start']);timeline.update();qt_app.processEvents()
    assert len(timeline.selection_rects)==1
    outline=timeline.selection_rects[0]
    assert outline.left()<=rect.left() and outline.right()>=rect.right()
    assert outline.top()<rect.top() and outline.bottom()>rect.bottom()
    timeline.deleteLater()


def test_projected_clip_remains_one_click_target_across_form_changes(qt_app):
    from datetime import date
    timeline=Timeline(SimpleNamespace(t=lambda s:s,preferences={'dark':True}));timeline.resize(1000,120)
    row=dict(id='clip-1',kind='clip',timeline_state='search',stage='',mode='working',event=False,start='2026-01-01T10:00:00Z',end='2026-01-01T11:00:00Z',last_seen='2026-01-01T10:59:00Z',confirmed_end=True,restored=True,source_rows=[{'stage':'province'},{'stage':'request_no_slots'}],checks=12)
    timeline.set_rows([row],date(2026,1,1),1);timeline.show();qt_app.processEvents()
    hit=next((rect,rows) for rect,rows in timeline.hit if rows[0].get('kind')=='clip')
    text=timeline.hover_text(hit[0].center())
    assert 'Восстановлено' in text and '12' in text
    assert len(hit[1])==1
    timeline.deleteLater()


def test_availability_marker_uses_promoted_date_and_offices(qt_app):
    from datetime import date
    timeline=Timeline(SimpleNamespace(t=lambda s:s,preferences={'dark':True}));timeline.resize(1000,120)
    row=dict(id='available-1',kind='availability',availability_kind='booking',timeline_state='available',stage='booking',mode='',event=True,start='2026-01-01T10:00:00Z',end='2026-01-01T10:00:00Z',last_seen='2026-01-01T10:02:00Z',confirmed_end=True,office_seen_at='2026-01-01T10:00:00Z',booking_seen_at='2026-01-01T10:02:00Z',offices=['Office A'])
    timeline.set_rows([row],date(2026,1,1),1);timeline.show();qt_app.processEvents()
    rect,rows=next((rect,rows) for rect,rows in timeline.hit if rows[0].get('kind')=='availability')
    text=timeline.hover_text(rect.center())
    assert 'Office A' in text and '11:02:00' in text
    assert len(rows)==1
    timeline.deleteLater()


def test_hover_highlights_whole_projected_clip_without_selecting(qt_app):
    from datetime import date
    from PySide6.QtCore import QEvent,QPointF,Qt
    from PySide6.QtGui import QMouseEvent
    timeline=Timeline(SimpleNamespace(t=lambda s:s,preferences={'dark':True}));timeline.resize(1000,120)
    row=dict(id='hover-clip',kind='clip',timeline_state='search',stage='',mode='working',event=False,start='2026-01-01T10:00:00Z',end='2026-01-01T11:00:00Z',last_seen='2026-01-01T10:59:00Z',confirmed_end=True,restored=False,checks=2)
    timeline.set_rows([row],date(2026,1,1),1);timeline.show();qt_app.processEvents()
    rect=next(rect for rect,rows in timeline.hit if rows[0].get('kind')=='clip')
    event=QMouseEvent(QEvent.Type.MouseMove,rect.center(),rect.center(),Qt.MouseButton.NoButton,Qt.MouseButton.NoButton,Qt.KeyboardModifier.NoModifier)
    qt_app.sendEvent(timeline,event);qt_app.processEvents()
    assert timeline.hover_key==timeline.row_key(row)
    assert timeline.selected_key is None
    timeline.deleteLater()


def test_midnight_clip_details_cover_whole_clip(qt_app):
    from datetime import date
    timeline=Timeline(SimpleNamespace(t=lambda s:s,preferences={'dark':True}));timeline.resize(1000,150)
    row=dict(id='overnight',kind='clip',timeline_state='search',stage='',mode='working',event=False,start='2026-01-01T22:00:00Z',end='2026-01-02T01:00:00Z',last_seen='2026-01-02T01:00:00Z',confirmed_end=True,restored=False,checks=7)
    timeline.set_rows([row],date(2026,1,1),2);timeline.show();qt_app.processEvents()
    parts=[(rect,sources) for rect,sources in timeline.hit if sources[0].get('kind')=='clip']
    assert len(parts)==2
    assert all(sources[0]['_display_start']==row['start'] and sources[0]['_display_end']==row['end'] for rect,sources in parts)
    assert '3 ч' in timeline.hover_text(parts[-1][0].center())
    timeline.deleteLater()
