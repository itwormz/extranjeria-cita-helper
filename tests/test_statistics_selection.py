from datetime import date
from types import SimpleNamespace


def clip(id, start, end, state='search'):
    return dict(id=id,kind='clip',timeline_state=state,stage='request_no_slots',mode='',start=f'2026-10-23T{start}:00Z',end=f'2026-10-23T{end}:00Z',last_seen=f'2026-10-23T{end}:00Z',confirmed_end=True,event=False,source_rows=[])


def test_selection_summary_unions_time_excludes_gaps_and_deduplicates_events():
    from app.desktop.statistics_selection import selection_summary
    a=clip('a','08:00','08:10');overlap=clip('overlap','08:05','08:15');gap=clip('gap','08:15','08:20','gap')
    event=dict(id=1,event=True,kind='check',start='2026-10-23T08:07:00Z',counted=True)
    ignored=dict(id=2,event=True,kind='check',start='2026-10-23T08:08:00Z',counted=False)
    office=dict(id=3,event=True,kind='office',start='2026-10-23T08:09:00Z',counted=True,offices=['Office A'])
    marker=dict(id='m',event=True,kind='availability',start=office['start'],source_rows=[office])
    result=selection_summary([a,overlap,gap,marker],[event,event,ignored,office])
    assert result['seconds']==1200
    assert result['observed_seconds']==900
    assert result['gap_seconds']==300
    assert result['counts']['checks']==1
    assert result['counts']['offices_seen']==1
    assert result['offices']==['Office A']


def test_selection_summary_does_not_fill_unselected_time_or_count_right_boundary():
    from app.desktop.statistics_selection import selection_summary
    rows=[clip('a','08:00','08:10'),clip('b','08:30','08:40')]
    events=[dict(id=n,event=True,kind='check',start=f'2026-10-23T08:{m}:00Z') for n,m in enumerate(['09','10','20','30'])]
    summary=selection_summary(rows,events)
    assert summary['seconds']==1200
    assert summary['counts']['checks']==2


def timeline(qt_app):
    from app.desktop.statistics_history import Timeline
    widget=Timeline(SimpleNamespace(t=lambda t:t,preferences={'dark':True,'language':'ru'}))
    widget.resize(1000,100);widget.set_rows([clip('a','08:00','09:00'),clip('b','09:00','10:00','cod'),clip('c','10:00','11:00')],date(2026,10,23),1)
    widget.show();qt_app.processEvents();return widget


def test_drag_selects_whole_clips_and_modifier_click_toggles(qt_app):
    from PySide6.QtCore import Qt,QPointF,QEvent
    from PySide6.QtGui import QMouseEvent
    from PySide6.QtTest import QTest
    widget=timeline(qt_app);selections=[];widget.selected.connect(selections.append)
    a=next(rect.center().toPoint() for rect,rows in widget.hit if rows[0].get('id')=='a')
    b=next(rect.center().toPoint() for rect,rows in widget.hit if rows[0].get('id')=='b')
    QTest.mousePress(widget,Qt.MouseButton.LeftButton,pos=a)
    qt_app.sendEvent(widget,QMouseEvent(QEvent.Type.MouseMove,QPointF(b),QPointF(widget.mapToGlobal(b)),Qt.MouseButton.NoButton,Qt.MouseButton.LeftButton,Qt.KeyboardModifier.NoModifier))
    QTest.mouseRelease(widget,Qt.MouseButton.LeftButton,pos=b)
    assert {row['id'] for row in selections[-1]}=={'a','b'}
    assert widget.view_start==0
    QTest.mouseClick(widget,Qt.MouseButton.LeftButton,Qt.KeyboardModifier.ControlModifier,pos=a)
    assert {row['id'] for row in selections[-1]}=={'b'}
    widget.close()


def test_markers_are_above_lane_and_hover_and_gap_are_visibly_highlighted(qt_app):
    from PySide6.QtCore import Qt,QPointF,QEvent
    from PySide6.QtGui import QMouseEvent
    from PySide6.QtTest import QTest
    widget=timeline(qt_app)
    def hover(point):
        qt_app.sendEvent(widget,QMouseEvent(QEvent.Type.MouseMove,QPointF(point),QPointF(widget.mapToGlobal(point)),Qt.MouseButton.NoButton,Qt.MouseButton.NoButton,Qt.KeyboardModifier.NoModifier));qt_app.processEvents()
    event=dict(id='office',kind='office',stage='office',mode='',start='2026-10-23T09:00:00Z',last_seen='2026-10-23T09:00:00Z',end=None,event=True,offices=['Office A'])
    widget.set_rows([clip('a','08:00','10:00'),event],date(2026,10,23),1);qt_app.processEvents()
    mark=next(rect for rect,rows in widget.hit if rows[0].get('id')=='office')
    lane=next(rect for rect,rows in widget.hit if rows[0].get('id')=='a')
    assert mark.bottom()<lane.top()
    before=widget.grab().toImage();hover(mark.center().toPoint())
    after=widget.grab().toImage();assert before.copy(mark.toRect())!=after.copy(mark.toRect())
    gap=next(rect for rect,rows in widget.hit if rows[0].get('mode')=='unobserved')
    hover(gap.center().toPoint())
    assert widget.hover_key is not None
    QTest.mouseClick(widget,Qt.MouseButton.LeftButton,pos=gap.center().toPoint());qt_app.processEvents()
    assert any(rect.intersects(gap) for rect in widget.selection_rects)
    widget.close()


def test_multiple_selection_details_and_refresh_keep_all_clips(qt_app):
    from app.desktop.statistics_history import StatisticsHistory
    rows=[clip('a','08:00','08:10'),clip('b','08:30','08:40','cod')]
    events=[dict(id=1,event=True,kind='check',start='2026-10-23T08:05:00Z',end='2026-10-23T08:05:00Z',last_seen='2026-10-23T08:05:00Z',confirmed_end=True)]
    class Store:
        def history_bounds(self,*a,**k):return {'start':rows[0]['start'],'end':rows[-1]['end']}
        def office_history(self,*a,**k):return []
        def timeline_history(self,*a,**k):return rows
        def history(self,*a,**k):return events
    panel=StatisticsHistory(SimpleNamespace(t=lambda s:s,preferences={'dark':True,'language':'ru'}),Store())
    panel.refresh('p','all');panel.show_details(rows)
    assert '20 мин' in panel.detail.text()
    assert 'Выбрано: 2' in panel.detail.text()
    panel._render_signature=None;panel.render_timeline()
    assert {r['id'] for r in panel.selected_rows}=={'a','b'}
    panel.deleteLater()


def test_drag_on_empty_background_pans_without_replacing_selection(qt_app):
    from PySide6.QtCore import Qt,QPointF,QEvent,QPoint
    from PySide6.QtGui import QMouseEvent
    from PySide6.QtTest import QTest
    widget=timeline(qt_app);widget.zoom(.5,anchor=.5);qt_app.processEvents()
    widget.set_selection([clip('a','08:00','09:00')]);old=widget.view_start
    a=QPoint(500,widget.height()-2);b=QPoint(550,widget.height()-2)
    QTest.mousePress(widget,Qt.MouseButton.LeftButton,pos=a)
    qt_app.sendEvent(widget,QMouseEvent(QEvent.Type.MouseMove,QPointF(b),QPointF(widget.mapToGlobal(b)),Qt.MouseButton.NoButton,Qt.MouseButton.LeftButton,Qt.KeyboardModifier.NoModifier))
    QTest.mouseRelease(widget,Qt.MouseButton.LeftButton,pos=b)
    assert widget.view_start<old
    assert [r['id'] for r in widget.selected_rows]==['a']
    widget.close()


def test_restored_navigation_is_not_reported_as_unknown_error():
    from app.desktop.statistics_selection import selection_summary
    normal=clip('normal','08:00','08:10')
    failure=clip('failure','08:10','08:15','recovery')
    raw=[dict(id=1,kind='page',event=False,stage='unknown',start='2026-10-23T08:05:00Z'),dict(id=2,kind='page',event=False,stage='unknown',start='2026-10-23T08:12:00Z')]
    assert selection_summary([normal],raw)['counts']['unknown_errors']==0
    assert selection_summary([normal,failure],raw)['counts']['unknown_errors']==1


def test_selected_gap_is_cleared_when_period_excludes_it(qt_app):
    from app.desktop.statistics_history import StatisticsHistory
    class Store:
        def history_bounds(self,*a,**k):return {'start':'2026-10-23T08:00:00Z','end':'2026-10-24T08:00:00Z'}
        def history(self,*a,**k):return []
        def office_history(self,*a,**k):return []
    panel=StatisticsHistory(SimpleNamespace(t=lambda s:s,preferences={'dark':True,'language':'ru'}),Store())
    panel.refresh('p','all');panel.show_details([clip('gap','08:00','09:00','gap')])
    panel.custom_range=(date(2026,10,24),date(2026,10,24));panel.render_timeline()
    assert panel.selected_rows==[]
    panel.deleteLater()


def test_drag_range_includes_raised_events_in_selected_row(qt_app):
    from PySide6.QtCore import QPointF
    widget=timeline(qt_app)
    event=dict(id='office',kind='office',stage='office',mode='',start='2026-10-23T09:00:00Z',last_seen='2026-10-23T09:00:00Z',end=None,event=True,offices=['Office A'])
    widget.set_rows([clip('a','08:00','09:00'),clip('b','09:00','10:00','cod'),event],date(2026,10,23),1);qt_app.processEvents()
    a=next(rect.center() for rect,rows in widget.hit if rows[0].get('id')=='a')
    b=next(rect.center() for rect,rows in widget.hit if rows[0].get('id')=='b')
    widget.select_drag_range(a,b)
    assert {r['id'] for r in widget.selected_rows}=={'a','b','office'}
    widget.close()


def test_new_observations_replace_selected_gap_across_repeated_hour(qt_app):
    from app.desktop.statistics_history import StatisticsHistory,segment_kind
    gap=dict(id='gap',kind='clip',timeline_state='gap',stage='',mode='',start='2026-10-25T01:00:00Z',end='2026-10-25T01:20:00Z',last_seen='2026-10-25T01:00:00Z',confirmed_end=False,event=False)
    observation=dict(id='observed',kind='clip',timeline_state='search',stage='',mode='',start='2026-10-25T00:55:00Z',end='2026-10-25T01:10:00Z',last_seen='2026-10-25T01:10:00Z',confirmed_end=True,event=False)
    class Store:
        rows=[]
        def history_bounds(self,*a,**k):return {'start':'2026-10-25T00:00:00Z','end':'2026-10-25T02:00:00Z'}
        def history(self,*a,**k):return self.rows
        def office_history(self,*a,**k):return []
    store=Store();panel=StatisticsHistory(SimpleNamespace(t=lambda s:s,preferences={'dark':True,'language':'ru'}),store)
    panel.refresh('p','all');panel.show_details([gap]);store.rows=[observation];panel._render_signature=None;panel.render_timeline()
    assert not any(segment_kind(row)=='gap' for row in panel.selected_rows)
    panel.deleteLater()


def test_grouped_office_outline_follows_measured_counter_width(qt_app):
    from app.desktop.statistics_history import Timeline
    widget=timeline(qt_app)
    events=[dict(id=n,kind='office',stage='office',mode='',start=f'2026-10-23T08:{n:02d}:00Z',last_seen=f'2026-10-23T08:{n:02d}:00Z',end=None,event=True,offices=[f'Office {n}']) for n in range(2)]
    widget.set_rows(events,date(2026,10,23),1);qt_app.processEvents()
    rect,rows=next((rect,rows) for rect,rows in widget.hit if rows[0].get('kind')=='office')
    assert len(rows)==2
    assert rect.width()<=18+widget.fontMetrics().horizontalAdvance('2')+5
    widget.close()
