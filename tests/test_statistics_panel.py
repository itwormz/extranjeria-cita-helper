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
    assert panel.success_button.text()=='Mark appointment as obtained'
    assert panel.first_value.text()=='Not seen yet'
    panel.statistics.observe('one','pass','start');panel.refresh()
    assert panel.stat_values['days'].text()=='Less than a day'
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
