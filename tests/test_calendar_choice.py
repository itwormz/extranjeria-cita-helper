from urllib.parse import urlparse,parse_qs
import pytest
from app.desktop.achievement_exports import google_calendar_url,write_calendar


def appointment(zone='Europe/Madrid'):
    return {'id':'demo','appointment':dict(date='2026-10-22',time='10:30',office='CNP EXAMPLE',address='Example street, 1',confirmation='SECRET-CODE',timezone=zone)}


@pytest.mark.parametrize('zone,utc',[('Europe/Madrid','20261022T083000Z'),('Atlantic/Canary','20261022T093000Z')])
def test_google_calendar_timezone_and_no_confirmation(zone,utc):
    url=google_calendar_url(appointment(zone));query=parse_qs(urlparse(url).query)
    assert query['dates']==[utc+'/'+utc];assert query['ctz']==[zone];assert query['location']==['CNP EXAMPLE, Example street, 1'];assert 'SECRET-CODE' not in url


def test_ics_never_contains_confirmation(tmp_path):
    file=write_calendar(appointment(),tmp_path/'event.ics');assert 'SECRET-CODE' not in file.read_text()


@pytest.mark.parametrize('language',['ru','en','es'])
def test_dialog_saves_only_after_selection_and_opens_google_explicitly(tmp_path,qt_app,monkeypatch,language):
    from types import SimpleNamespace
    from PySide6.QtWidgets import QMainWindow,QFileDialog
    from PySide6.QtGui import QDesktopServices
    from app.desktop.i18n import translate
    from app.desktop.calendar_dialog import CalendarDialog
    host=QMainWindow();host.preferences={'language':language,'dark':True};host.t=lambda text:translate(text,language);calls=[];monkeypatch.setattr(QDesktopServices,'openUrl',lambda url:calls.append(url.toString()) or True)
    dialog=CalendarDialog(host,appointment());assert calls==[]
    monkeypatch.setattr(QFileDialog,'getSaveFileName',lambda *a,**kw:('', ''));dialog.save_file();assert calls==[];assert dialog.saved_path is None
    file=tmp_path/'event.ics';monkeypatch.setattr(QFileDialog,'getSaveFileName',lambda *a,**kw:(str(file),'iCalendar'));dialog.save_file();assert file.is_file();assert calls==[];assert 'SECRET-CODE' not in file.read_text()
    dialog.open_file();assert calls[-1].startswith('file:');dialog.open_google();assert calls[-1].startswith('https://calendar.google.com/');assert all('SECRET-CODE' not in url for url in calls)
    dialog.deleteLater();host.deleteLater()
