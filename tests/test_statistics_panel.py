import os
os.environ.setdefault('QT_QPA_PLATFORM','offscreen')
from types import SimpleNamespace
from PySide6.QtWidgets import QMainWindow, QTabWidget
from app.desktop.i18n import translate

def test_profile_statistics_selector_is_independent_of_running_profile(tmp_path,qt_app):
    from app.desktop.statistics_panel import StatisticsPanel
    from app.desktop.statistics import ProfileStatistics
    host=QMainWindow();host.store=SimpleNamespace(path=tmp_path/'tasks.yml',load=lambda:[SimpleNamespace(id='one',label='One',province='Barcelona'),SimpleNamespace(id='two',label='Two',province='Barcelona')]);host.controller=SimpleNamespace(active=True,send=lambda *_:None);host.preferences={'language':'en','dark':True};host.t=lambda text:translate(text,'en')
    stats=ProfileStatistics(tmp_path/'data/profile-statistics.json');stats.increment('one');stats.increment('two');stats.increment('two')
    panel=StatisticsPanel(host);panel.set_profiles(host.store.load(),'one');panel.profile_combo.setCurrentIndex(1);panel.refresh()
    assert panel.profile_combo.currentData()=='two'
    assert panel.stat_values['checks'].text()=='2'
    assert host.controller.active
    panel.deleteLater();host.deleteLater()

def make_host(tmp_path,profiles):
    host=QMainWindow();host.store=SimpleNamespace(path=tmp_path/'tasks.yml',load=lambda:profiles);host.preferences={'language':'en','dark':True};host.t=lambda text:translate(text,'en');host.commands=[];host.controller=SimpleNamespace(active=True,profile_id='one',send=host.commands.append);host.state={'selected_office':'Office of running profile'};return host


def test_success_for_other_profile_does_not_copy_office_or_stop_search(tmp_path,qt_app,monkeypatch):
    from app.desktop.statistics_panel import StatisticsPanel
    from PySide6.QtWidgets import QDialog,QLineEdit
    profiles=[SimpleNamespace(id='one',label='One',province='Barcelona'),SimpleNamespace(id='two',label='Two',province='Barcelona')]
    host=make_host(tmp_path,profiles);panel=StatisticsPanel(host);panel.set_profiles(profiles,'two');office=[]
    def inspect(dialog):
        office.append(dialog.findChild(QLineEdit,"appointmentOffice").text());return QDialog.DialogCode.Rejected
    monkeypatch.setattr(QDialog,'exec',inspect);panel.confirm_success()
    assert office==[''];assert host.commands==[]
    panel.deleteLater();host.deleteLater()


def test_private_edit_keeps_snapshot_and_calendar_identity(tmp_path,qt_app,monkeypatch):
    from app.desktop.statistics_panel import StatisticsPanel
    from app.desktop.achievement_exports import write_calendar
    from PySide6.QtWidgets import QDialog,QLineEdit
    profiles=[SimpleNamespace(id='one',label='One',province='Barcelona')];host=make_host(tmp_path,profiles);panel=StatisticsPanel(host);panel.set_profiles(profiles,'one')
    original=panel.achievements.record('one',{'checks':42},{'date':'2026-10-22','time':'09:00','office':'Original office','address':'Test address','confirmation':'DEMO','timezone':'Europe/Madrid'})
    panel.current_achievement=original;before=write_calendar(original,tmp_path/'before.ics').read_text()
    def edit(dialog):
        dialog.findChild(QLineEdit,"appointmentOffice").setText('Corrected office');return QDialog.DialogCode.Accepted
    monkeypatch.setattr(QDialog,'exec',edit);panel.edit_appointment();after=panel.current_achievement
    assert after['id']==original['id'];assert after['statistics']==original['statistics'];assert after['appointment']['office']=='Corrected office';assert len(panel.achievements.list('one'))==1;assert host.commands==[]
    calendar=write_calendar(after,tmp_path/'after.ics').read_text();assert [l for l in before.splitlines() if l.startswith('UID:')]==[l for l in calendar.splitlines() if l.startswith('UID:')]
    panel.deleteLater();host.deleteLater()


def test_confirm_success_stops_only_matching_profile_and_keeps_snapshot(tmp_path,qt_app,monkeypatch):
    from app.desktop.statistics_panel import StatisticsPanel
    from PySide6.QtWidgets import QDialog
    profiles=[SimpleNamespace(id="one",label="One",province="Barcelona")];host=make_host(tmp_path,profiles);panel=StatisticsPanel(host);panel.set_profiles(profiles,"one")
    panel.statistics.increment("one")
    monkeypatch.setattr(QDialog,"exec",lambda dialog: QDialog.DialogCode.Accepted)
    panel.confirm_success()
    assert host.commands==[{"command":"stop"}]
    assert panel.current_achievement["statistics"]["checks"]==1
    assert panel.current_achievement["appointment"]["office"]=="Office of running profile"
    assert panel.stack.currentWidget()==panel.achievement_page
    panel.statistics.increment("one")
    assert panel.current_achievement["statistics"]["checks"]==1
    panel.deleteLater();host.deleteLater()


def test_appointment_form_offices_are_profile_scoped_and_prefill_address(tmp_path,qt_app,monkeypatch):
    from app.desktop.statistics_panel import StatisticsPanel
    from PySide6.QtWidgets import QDialog,QLineEdit,QComboBox,QDateEdit,QTimeEdit,QAbstractSpinBox,QPushButton
    profiles=[SimpleNamespace(id='one',label='One',province='Barcelona')];host=make_host(tmp_path,profiles);host.state={};panel=StatisticsPanel(host);panel.set_profiles(profiles,'one')
    panel.statistics.observe('one','office','doc',offices=['Barcelona: CNP EXAMPLE, Example street, 1, BARCELONA'])
    panel.statistics.observe('other','office','doc',offices=['Barcelona: OTHER OFFICE, Other street'])
    def inspect(dialog):
        combo=dialog.findChild(QComboBox);assert combo.count()==1;assert 'OTHER' not in combo.itemText(0)
        assert dialog.office.currentText()=='CNP EXAMPLE'
        assert dialog.address.text()=='Example street, 1, BARCELONA'
        combo.setCurrentIndex(0);combo.activated.emit(0);assert dialog.findChild(QLineEdit,'appointmentAddress').text()=='Example street, 1, BARCELONA'
        dialog.show();qt_app.processEvents();assert dialog.date.width()==98;assert dialog.time.width()==58
        assert dialog.office.currentText()=='CNP EXAMPLE'
        texts=[b.text() for b in dialog.findChildren(QPushButton)];assert texts.count('Cancel')==1;assert 'Save' in texts
        return QDialog.DialogCode.Rejected
    monkeypatch.setattr(QDialog,'exec',inspect);panel.confirm_success();panel.deleteLater();host.deleteLater()


def test_achievements_without_record_has_accessible_empty_state(tmp_path,qt_app):
    from app.desktop.statistics_panel import StatisticsPanel
    host=make_host(tmp_path,[SimpleNamespace(id='one',label='One',province='Barcelona')]);panel=StatisticsPanel(host);panel.set_profiles(host.store.load(),'one')
    assert panel.achievement_button.isHidden();panel.show_achievement();assert panel.stack.currentWidget()==panel.empty_page;assert panel.current_achievement is None
    panel.deleteLater();host.deleteLater()


def test_empty_achievement_returns_without_inheriting_hidden_card_height(tmp_path,qt_app):
    from app.desktop.statistics_panel import StatisticsPanel
    from PySide6.QtWidgets import QScrollArea
    profiles=[SimpleNamespace(id='one',label='One',province='Barcelona')];host=make_host(tmp_path,profiles);panel=StatisticsPanel(host);scroll=QScrollArea();scroll.setWidgetResizable(True);scroll.setWidget(panel);host.setCentralWidget(scroll);host.resize(820,620);panel.set_profiles(profiles,'one');host.show();panel.show_achievement();qt_app.processEvents()
    assert scroll.verticalScrollBar().maximum()==0
    assert panel.back_button.isVisible();panel.back_button.click();qt_app.processEvents()
    assert panel.stack.currentWidget()==panel.stats_page;assert not panel.back_button.isVisible()
    host.close();host.deleteLater()


def test_statistics_actions_and_first_availability_copy(tmp_path,qt_app):
    from app.desktop.statistics_panel import StatisticsPanel
    host=make_host(tmp_path,[SimpleNamespace(id='one',label='One',province='Barcelona')])
    panel=StatisticsPanel(host);panel.set_profiles(host.store.load(),'one')
    assert panel.stats_hint.isHidden()
    assert panel.achievement_button.isHidden()
    assert panel.success_button.text()=='Sita obtained'
    assert panel.first_value.text()=='Not seen yet'
    panel.statistics.observe('one','pass','start');panel.refresh()
    assert panel.stat_values['days'].text()=='1'
    panel.achievements.record('one',{'checks':1},{'date':'2026-10-22','time':'09:00','office':'Test office'})
    panel.refresh();assert not panel.achievement_button.isHidden()
    panel.deleteLater();host.deleteLater()


def test_saved_appointment_values_can_be_selected_and_copied(tmp_path,qt_app):
    from app.desktop.statistics_panel import StatisticsPanel
    from PySide6.QtCore import Qt
    from PySide6.QtWidgets import QApplication
    profiles=[SimpleNamespace(id='one',label='One',province='Barcelona')]
    host=make_host(tmp_path,profiles);panel=StatisticsPanel(host);panel.set_profiles(profiles,'one')
    panel.achievements.record('one',{'checks':1},{'date':'2026-10-22','time':'09:00','office':'Office A','address':'Street A','confirmation':'DEMO-123','timezone':'Europe/Madrid'})
    panel.show_achievement()
    panel.refresh()
    for key in ('office','confirmation'):
        field=panel.private_fields[key]
        assert field.textInteractionFlags() & Qt.TextInteractionFlag.TextSelectableByMouse
        assert field.textInteractionFlags() & Qt.TextInteractionFlag.TextSelectableByKeyboard
    assert panel.private_fields['office'].text()=='Office A\nStreet A'
    assert panel.private_fields['confirmation'].text()=='DEMO-123'
    panel.deleteLater();host.deleteLater()


def test_period_changes_all_metrics_and_history_and_warning(tmp_path,qt_app):
    from app.desktop.statistics_panel import StatisticsPanel
    from datetime import datetime,timedelta,timezone
    host=make_host(tmp_path,[SimpleNamespace(id='one',label='One',province='Barcelona')])
    host.state['statistics_error']=True
    panel=StatisticsPanel(host);panel.set_profiles(host.store.load(),'one')
    panel.statistics.increment('one',at=datetime.now(timezone.utc)-timedelta(days=3))
    panel.statistics.observe('one','office','old',offices=['Old office'],at=datetime.now(timezone.utc)-timedelta(days=3))
    panel.refresh();assert panel.stat_values['checks'].text()=='1'
    assert not panel.statistics_warning.isHidden()
    panel.today_button.click()
    assert panel.stat_values['checks'].text()=='0'
    assert panel.history_panel.period=='today'
    assert not panel.history_panel.office_rows
    assert panel.first_booking_value.text()=='Not seen yet'
    from app.desktop.theme import apply_theme
    apply_theme(qt_app,dark=True);panel.show();qt_app.processEvents()
    assert panel.success_button.minimumHeight()==38
    assert panel.success_button.height()==38
    panel.deleteLater();host.deleteLater()


def test_today_counts_recorded_appointments_and_baseline_has_no_fake_days(tmp_path,qt_app):
    from app.desktop.statistics_panel import StatisticsPanel
    host=make_host(tmp_path,[SimpleNamespace(id='one',label='One',province='Barcelona')])
    panel=StatisticsPanel(host);panel.set_profiles(host.store.load(),'one')
    panel.achievements.record('one',{'checks':100},{'date':'2026-11-01','office':'Test'})
    panel.today_button.click()
    assert panel.stat_values['successes'].text()=='1'
    assert panel.stat_values['days'].text()=='0'
    panel.deleteLater();host.deleteLater()


def test_offices_and_first_events_follow_profile_period(tmp_path,qt_app):
    from app.desktop.statistics_panel import StatisticsPanel
    from datetime import datetime,timezone,timedelta
    host=make_host(tmp_path,[SimpleNamespace(id='one',label='One',province='Barcelona'),SimpleNamespace(id='two',label='Two',province='Barcelona')])
    panel=StatisticsPanel(host);panel.set_profiles(host.store.load(),'one')
    panel.statistics.observe('one','office','a',offices=['Office A'])
    panel.statistics.observe('two','office','b',offices=['Office B'])
    panel.statistics.observe('one','booking','c')
    panel.refresh()
    assert [row['label'] for row in panel.history_panel.office_rows]==['Office A']
    assert panel.first_booking_value.text()!='Not seen yet'
    panel.profile_combo.setCurrentIndex(1)
    assert [row['label'] for row in panel.history_panel.office_rows]==['Office B']
    assert panel.first_booking_value.text()=='Not seen yet'
    panel.deleteLater();host.deleteLater()


def test_legacy_totals_do_not_claim_unknown_dates_or_zero_known_days(tmp_path,qt_app):
    import json
    from app.desktop.statistics_panel import StatisticsPanel
    target=tmp_path/'data/profile-statistics.json';target.parent.mkdir()
    target.write_text(json.dumps({'one':{'checks':100,'offices_seen':3,'booking_screens':2}}))
    host=make_host(tmp_path,[SimpleNamespace(id='one',label='One',province='Barcelona')]);panel=StatisticsPanel(host);panel.set_profiles(host.store.load(),'one')
    assert panel.stat_values['checks'].text()=='100'
    assert panel.stat_values['days'].text()=='—'
    assert panel.first_value.text()=='Date unknown'
    assert panel.first_booking_value.text()=='Date unknown'
    panel.deleteLater();host.deleteLater()


def test_first_recorded_duration_uses_minimum_and_keeps_unknown_in_tooltip(tmp_path,qt_app):
    import json
    from datetime import datetime,timedelta,timezone
    from app.desktop.statistics_panel import StatisticsPanel
    target=tmp_path/'data/profile-statistics.json';target.parent.mkdir()
    target.write_text(json.dumps({'one':{'checks':100,'offices_seen':1,'active_seconds':1000}}))
    host=make_host(tmp_path,[SimpleNamespace(id='one',label='One',province='Barcelona')])
    panel=StatisticsPanel(host);panel.set_profiles(host.store.load(),'one')
    at=datetime.now(timezone.utc)
    panel.statistics.add_observed_active('one',at-timedelta(seconds=65),at,session_id='archive-test')
    panel.statistics.observe('one','office','new-office',offices=['Office'],at=at)
    panel.refresh()
    assert '≥ 1 min' in panel.first_value.text()
    assert 'recorded' in panel.first_caption.text().lower()
    assert 'unknown' not in panel.first_value.text().lower()
    assert panel.statistics.get('one')['active_seconds']==1000
    panel.statistics.observe('one','booking','date',at=at)
    panel.refresh();assert '≥ 1 min' in panel.first_booking_value.text()
    panel.deleteLater();host.deleteLater()


def test_subminute_first_duration_is_shown_in_seconds(tmp_path,qt_app):
    from datetime import datetime,timedelta,timezone
    from app.desktop.statistics_panel import StatisticsPanel
    host=make_host(tmp_path,[SimpleNamespace(id='one',label='One',province='Barcelona')])
    panel=StatisticsPanel(host);panel.set_profiles(host.store.load(),'one');at=datetime.now(timezone.utc)
    panel.statistics.add_observed_active('one',at-timedelta(seconds=56),at,session_id='archive-test')
    panel.statistics.observe('one','office','new-office',offices=['Office'],at=at);panel.refresh()
    assert '≥ 56 s' in panel.first_value.text()
    assert '0 min' not in panel.first_value.text()
    panel.deleteLater();host.deleteLater()


def test_effort_values_show_unknown_then_persist_today_and_all(tmp_path,qt_app):
    from app.desktop.statistics_panel import StatisticsPanel
    profiles=[SimpleNamespace(id='one',label='One',province='Test')]
    host=make_host(tmp_path,profiles);panel=StatisticsPanel(host);panel.set_profiles(profiles,'one')
    assert panel.stat_values['actions_performed'].text()=='—'
    panel.statistics.event('one','automation_action','old',at='2020-01-01T12:00:00+00:00')
    panel.statistics.event('one','automation_action','today')
    for _ in range(2):panel.statistics.event('one','processed_page','same-document')
    panel.period='all';panel.refresh()
    assert panel.stat_values['actions_performed'].text()=='2'
    assert panel.stat_values['pages_processed'].text()=='1'
    panel.period='today';panel.refresh()
    assert panel.stat_values['actions_performed'].text()=='1'
    assert panel.stat_values['pages_processed'].text()=='1'
    panel.deleteLater();host.deleteLater()


def test_automation_section_follows_availability_and_errors(tmp_path,qt_app):
    from app.desktop.statistics_panel import StatisticsPanel
    from PySide6.QtWidgets import QLabel
    profiles=[SimpleNamespace(id='one',label='One',province='Test')]
    host=make_host(tmp_path,profiles);panel=StatisticsPanel(host);panel.set_profiles(profiles,'one')
    panel.resize(1050,1000);panel.show();qt_app.processEvents()
    heading=next(label for label in panel.findChildren(QLabel) if label.text()=='Automation')
    y=lambda widget:widget.mapTo(panel,widget.rect().center()).y()
    assert y(heading)>y(panel.stat_values['blocked'])
    assert y(panel.stat_values['actions_performed'])>y(heading)
    assert y(panel.stat_values['pages_processed'])==y(panel.stat_values['actions_performed'])
    assert 'pointer' in [name for widget,name in panel._icon_bindings]
    assert 'file_text' in [name for widget,name in panel._icon_bindings]
    panel.close();panel.deleteLater();host.deleteLater()


def test_missing_statistics_sidecar_has_no_revision(tmp_path,monkeypatch):
    from pathlib import Path
    from app.desktop.statistics_history import history_revision
    from app.desktop.statistics_panel import _file_revision
    database=tmp_path/'profile-statistics.sqlite3'
    database.write_bytes(b'x')
    (tmp_path/'profile-statistics.sqlite3-wal').write_bytes(b'yy')
    achievements=tmp_path/'achievements.json'
    achievements.write_text('{}',encoding='utf-8')
    real=Path.stat
    def vanish(self,*args,**kwargs):
        if self.name.endswith('-wal') or self.name=='achievements.json':raise FileNotFoundError(self)
        return real(self,*args,**kwargs)
    monkeypatch.setattr(Path,'stat',vanish)
    recorded,wal=history_revision(SimpleNamespace(db_path=database))
    assert recorded[1]==1 and wal is None
    assert _file_revision(achievements) is None


def test_finished_read_keeps_a_newer_custom_range(tmp_path,qt_app):
    from datetime import date
    from app.desktop.statistics_panel import StatisticsPanel
    profiles=[SimpleNamespace(id='one',label='One',province='Barcelona')]
    host=make_host(tmp_path,profiles);panel=StatisticsPanel(host);panel.set_profiles(profiles,'one');panel.refresh()
    history=panel.history_panel
    chosen=(date(2026,1,1),date(2026,1,2))
    history.custom_range=chosen;history.render_timeline()
    label=history.period_button.text()
    panel._consume(('ok',panel.statistics.get('one'),[],{'bounds':None,'office_rows':[],'rows':[],'raw_rows':[]},(('one','all',None),None,None),'one','all',None))
    assert history.custom_range==chosen and history.period_button.text()==label
    assert panel._applied_revision is None
    panel.statistics.increment('one')
    panel._consume(('ok',panel.statistics.get('one'),[],{'bounds':None,'office_rows':[],'rows':[],'raw_rows':[]},(('one','all',None),None,None),'one','all',chosen))
    assert panel.stat_values['checks'].text()=='1'
    panel.deleteLater();host.deleteLater()


def test_read_revision_changed_during_load_stays_unstable(tmp_path):
    from app.desktop.statistics_panel import _UNSTABLE_REVISION, load_statistics_view
    database=tmp_path/'profile-statistics.sqlite3';database.write_bytes(b'a')
    achievements=tmp_path/'achievements.json';achievements.write_text('{}',encoding='utf-8')
    class Statistics:
        db_path=database
        def get(self,*args,**kwargs):
            database.write_bytes(b'ab');return {'checks':0}
        def timeline_bounds(self,*args,**kwargs):return None
        history_bounds=timeline_bounds
        def office_history(self,*args,**kwargs):return []
        def timeline_history(self,*args,**kwargs):return []
        def history(self,*args,**kwargs):return []
    class Achievements:
        path=achievements
        def list(self,*args,**kwargs):
            achievements.write_text('{"changed":true}',encoding='utf-8');return []
    result=load_statistics_view(Statistics(),Achievements(),'one','all',None,('one','all',None))
    assert result[0]=='ok'
    assert result[4][1] is _UNSTABLE_REVISION and result[4][2] is _UNSTABLE_REVISION


def test_profile_change_does_not_keep_the_previous_totals(tmp_path,qt_app,monkeypatch):
    from app.desktop.statistics_panel import StatisticsPanel
    profiles=[SimpleNamespace(id='one',label='One',province='Barcelona'),SimpleNamespace(id='two',label='Two',province='Barcelona')]
    host=make_host(tmp_path,profiles);panel=StatisticsPanel(host);panel.set_profiles(profiles,'one')
    panel.statistics.increment('one');panel.refresh()
    assert panel.stat_values['checks'].text()=='1'
    def unavailable(*args,**kwargs):raise OSError('temporary read failure')
    monkeypatch.setattr(panel.statistics,'get',unavailable)
    panel.profile_combo.setCurrentIndex(panel.profile_combo.findData('two'))
    assert panel.profile_combo.currentData()=='two'
    assert panel.stat_values['checks'].text()=='—'
    panel.deleteLater();host.deleteLater()


def test_timeline_projection_does_not_hold_the_store_lock(tmp_path,monkeypatch):
    import threading
    from app.desktop.statistics import ProfileStatistics
    stats=ProfileStatistics(tmp_path/'profile-statistics.json')
    held=[]
    def project(rows,sessions=(),*,cache=None):
        def try_lock():
            acquired=stats._lock.acquire(blocking=False)
            held.append((acquired,cache))
            if acquired:stats._lock.release()
        thread=threading.Thread(target=try_lock);thread.start();thread.join()
        return []
    monkeypatch.setattr('app.desktop.statistics_timeline.build_timeline',project)
    stats.timeline_history('one')
    assert held==[(True,None)]


def test_statistics_refresh_reads_outside_the_interface_thread(tmp_path,qt_app,monkeypatch):
    import threading
    from app.desktop.statistics_panel import StatisticsPanel
    profiles=[SimpleNamespace(id='one',label='One',province='Barcelona')]
    host=make_host(tmp_path,profiles);panel=StatisticsPanel(host);panel.set_profiles(profiles,'one')
    seen=[]
    original=panel.statistics.get
    def wrapped(*args,**kwargs):
        seen.append(threading.get_ident());return original(*args,**kwargs)
    monkeypatch.setattr(panel.statistics,'get',wrapped)
    panel.statistics.increment('one');panel.refresh()
    assert seen and seen[0]!=threading.get_ident()
    assert panel.stat_values['checks'].text()=='1'
    panel.deleteLater();host.deleteLater()


def test_transient_statistics_read_error_clears_after_successful_refresh(tmp_path,qt_app,monkeypatch,caplog):
    from app.desktop.statistics_panel import StatisticsPanel
    profiles=[SimpleNamespace(id='one',label='One',province='Barcelona')]
    host=make_host(tmp_path,profiles);panel=StatisticsPanel(host);panel.set_profiles(profiles,'one')
    read=panel.statistics.get
    def unavailable(*args,**kwargs):raise OSError('temporary read failure')
    monkeypatch.setattr(panel.statistics,'get',unavailable);panel.refresh()
    assert panel.message.text()==host.t('Не удалось прочитать статистику профиля')
    failures=[record for record in caplog.records if getattr(record,'event',None)=='profile_statistics_read_failed']
    assert len(failures)==1 and failures[0].fields=={'component':'totals','error_type':'OSError'}
    assert 'temporary read failure' not in caplog.text
    monkeypatch.setattr(panel.statistics,'get',read);panel.refresh()
    assert panel.message.text()==''
    panel.deleteLater();host.deleteLater()
