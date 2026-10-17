from app.desktop.appointment_form import format_date, format_time, split_office


def test_date_formats_and_checks_actual_calendar():
    assert format_date('22102026')=='22.10.2026'
    assert format_date('1.2.2026')=='01.02.2026'
    assert format_date('29022028')=='29.02.2028'
    assert format_date('29022026') is None
    assert format_date('31112026') is None
    assert format_date('2210') is None


def test_time_never_guesses_incomplete_values():
    assert format_time('0930')=='09:30'
    assert format_time('9:30')=='09:30'
    assert format_time('23:59')=='23:59'
    assert format_time('24:00') is None
    assert format_time('09:60') is None
    assert format_time('09') is None


def test_office_address_preserves_original_parts():
    assert split_office('CNP COMISARIA GRANOLLERS, RICOMA, 65, GRANOLLERS')==('CNP COMISARIA GRANOLLERS','RICOMA, 65, GRANOLLERS')
    assert split_office('Office without address')==('Office without address','')


def test_form_blocks_invalid_save_and_formats_on_blur(qt_app):
    from types import SimpleNamespace
    from PySide6.QtWidgets import QMainWindow
    from app.desktop.appointment_form import AppointmentForm
    host=QMainWindow();host.preferences={'dark':True,'language':'en'};host.t=lambda text:text
    form=AppointmentForm(host,[]);form.date.setText('31112026');form.time.setText('0930');assert not form.save.isEnabled()
    form.accept();assert form.result()==0;assert not form.date_error.isHidden()
    form.date.setText('29022028');form.validate('date',True,True);form.validate('time',True,True)
    assert form.date.text()=='29.02.2028';assert form.time.text()=='09:30';assert form.save.isEnabled();form.accept();assert form.result()==1
    form.deleteLater();host.deleteLater()


def test_calendar_popup_keeps_form_geometry(qt_app):
    from types import SimpleNamespace
    from PySide6.QtWidgets import QMainWindow
    from PySide6.QtCore import QDate
    from app.desktop.appointment_form import AppointmentForm
    host=QMainWindow();host.preferences={'dark':True,'language':'en'};host.t=lambda text:text
    form=AppointmentForm(host,[]);form.show();qt_app.processEvents();size=form.size();form.open_calendar();qt_app.processEvents();assert form.size()==size;assert form.calendar_popup.isVisible()
    form.pick_date(QDate(2028,2,29));assert form.date.text()=='29.02.2028';assert not form.calendar_popup.isVisible()
    form.close();form.deleteLater();host.deleteLater()


def test_passes_are_not_expired_session_counts(tmp_path):
    from app.desktop.statistics import ProfileStatistics
    store=ProfileStatistics(tmp_path/'stats.json');store.observe('one','expired','error');assert store.get('one')['flow_passes']==0
    store.observe('one','pass','entry');store.observe('one','pass','entry');assert store.get('one')['flow_passes']==1
    store.observe('one','pass','new-entry');assert store.get('one')['flow_passes']==2


def test_live_separators_and_backspace(qt_app):
    from app.desktop.appointment_form import SeparatedDateTimeEdit
    from PySide6.QtTest import QTest
    from PySide6.QtCore import Qt
    time=SeparatedDateTimeEdit('time');QTest.keyClicks(time,'09');assert time.text()=='09:';QTest.keyClicks(time,'30');assert time.text()=='09:30'
    QTest.keyClick(time,Qt.Key.Key_Backspace);assert time.text()=='09:3';QTest.keyClick(time,Qt.Key.Key_Backspace);assert time.text()=='09:';QTest.keyClick(time,Qt.Key.Key_Backspace);assert time.text()=='0'
    date=SeparatedDateTimeEdit('date');QTest.keyClicks(date,'12');assert date.text()=='12.';QTest.keyClicks(date,'10');assert date.text()=='12.10.';QTest.keyClicks(date,'2026');assert date.text()=='12.10.2026'
    time.setText("09:30");time.setCursorPosition(2);QTest.keyClick(time,Qt.Key.Key_Delete);assert time.text()=="09:0"
    time.deleteLater();date.deleteLater()


def test_live_input_paste_and_middle_replacement(qt_app):
    from app.desktop.appointment_form import SeparatedDateTimeEdit
    from PySide6.QtWidgets import QApplication
    from PySide6.QtTest import QTest
    time=SeparatedDateTimeEdit('time');QApplication.clipboard().setText('0930');time.paste();assert time.text()=='09:30'
    time.setSelection(0,2);QTest.keyClicks(time,'12');assert time.text()=='12:30';assert time.cursorPosition()==3
    date=SeparatedDateTimeEdit('date');QApplication.clipboard().setText('22102026');date.paste();assert date.text()=='22.10.2026'
    time.deleteLater();date.deleteLater()


def test_last_observed_office_persists_per_profile(tmp_path):
    from app.desktop.statistics import ProfileStatistics
    path=tmp_path/'stats.json';store=ProfileStatistics(path)
    store.observe('one','office','a',offices=['P: Office Z, Street Z'])
    store.observe('one','office','b',offices=['P: Office A, Street A'])
    assert ProfileStatistics(path).last_office('one')=='P: Office A, Street A'
    assert store.last_office('other')==''
