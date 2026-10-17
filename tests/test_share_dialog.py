from types import SimpleNamespace
import pytest
from PySide6.QtWidgets import QMainWindow,QApplication,QPushButton
from app.desktop.achievements import AchievementStore
from app.desktop.i18n import translate
from app.desktop.share_dialog import ShareDialog


@pytest.mark.parametrize('language',['ru','en','es'])
@pytest.mark.parametrize('network',['telegram','x','facebook','instagram'])
def test_share_modes_prepare_locally_for_all_networks(tmp_path,qt_app,monkeypatch,language,network):
    from PySide6.QtGui import QDesktopServices
    calls=[];monkeypatch.setattr(QDesktopServices,'openUrl',lambda url:calls.append(url.toString()) or True)
    host=QMainWindow();host.store=SimpleNamespace(path=tmp_path/'profiles.yml');host.preferences={'language':language,'dark':True};host.t=lambda text:translate(text,language)
    item=AchievementStore(tmp_path/'achievements.json').record('demo',{'checks':42,'flow_passes':7,'active_seconds':3600},{'date':'2026-10-22','time':'09:30','office':'PRIVATE OFFICE','confirmation':'PRIVATE CONFIRMATION'})
    dialog=ShareDialog(host,item,network);text=dialog.text.toPlainText();assert '42' in text;assert 'PRIVATE' not in text;assert 'https://' not in text;assert 'github' not in text
    dialog.prepare();assert QApplication.clipboard().text()==text;assert len(calls)==1
    dialog.select_mode('card');dialog.prepare();assert dialog.prepared_path.is_file();assert dialog.prepared_path.parent==tmp_path/'data/share';assert len(calls)==2;assert calls[0]==calls[1]
    assert dialog.buttons['card'].isChecked();assert dialog.stack.currentIndex()==1;assert not dialog.show_file.isHidden()
    dialog.close();dialog.deleteLater();host.deleteLater()


def test_failed_png_never_opens_network(tmp_path,qt_app,monkeypatch):
    from PySide6.QtGui import QDesktopServices
    host=QMainWindow();host.store=SimpleNamespace(path=tmp_path/'profiles.yml');host.preferences={'language':'en','dark':False};host.t=lambda text:translate(text,'en');item=AchievementStore(tmp_path/'achievements.json').record('demo',{},{});dialog=ShareDialog(host,item,'x');dialog.select_mode('card')
    (tmp_path/'data').write_text('not a directory');calls=[];monkeypatch.setattr(QDesktopServices,'openUrl',lambda url:calls.append(url) or True);dialog.prepare();assert calls==[];assert dialog.prepared_path is None
    dialog.deleteLater();host.deleteLater()
