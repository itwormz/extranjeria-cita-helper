import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
import pytest
pytest.importorskip("PySide6")
from PySide6.QtWidgets import QApplication
from PySide6.QtCore import QObject, Signal, Qt
from PySide6.QtTest import QTest
from app.desktop.window import DesktopWindow
from app.desktop.profiles import ProfileStore

class Controller(QObject):
    state_changed = Signal(dict)
    failed = Signal(str)
    finished = Signal(int)
    active = False
    def start(self, profile_id): self.active = True
    def send(self, message): pass
    def shutdown(self, *, close_browser=False):
        self.close_browser=close_browser
        self.active = False

@pytest.fixture
def window(tmp_path, catalog_path, qt_app):
    app = qt_app
    store = ProfileStore(tmp_path / "tasks.yml", catalog_path)
    w = DesktopWindow(store, Controller()); w.show(); app.processEvents()
    yield w
    w.controller.active = False
    w.close(); w.deleteLater()
    from PySide6.QtCore import QCoreApplication, QEvent
    QCoreApplication.sendPostedEvents(None, QEvent.Type.DeferredDelete)
    app.processEvents()
    from shiboken6 import isValid
    assert not isValid(w), "GUI fixture must destroy the closed window before the next test"

def test_three_tabs_and_profile_creation_requires_valid_data(window):
    assert window.tabs.count() == 3
    assert not window.start_button.isEnabled()
    window.create_profile()
    assert window.fields['delay_min'].value()==.3
    assert window.fields['delay_max'].value()==.8
    assert window.profile_editor.isVisible()
    assert window.fields["label"].text() == ""
    for key,value in dict(label="Демонстрация",full_name="NOMBRE APELLIDO",document_number="X0000000T",phone="600000000",email="demo@example.com").items():
        window.fields[key].setText(value)
    window.fields["nationality"].setCurrentText("UCRANIA")
    window.save_profile()
    assert window.profile_combo.count() == 1
    assert window.start_button.isEnabled()
    assert not window.profile_editor.isVisible()

def test_dark_theme_and_toggles_persist_with_keyboard(window):
    before = window.palette().window().color().lightness()
    window.tabs.setCurrentIndex(2)
    window.dark_toggle.setFocus()
    QTest.keyClick(window.dark_toggle, Qt.Key.Key_Space)
    QApplication.processEvents()
    assert window.dark_toggle.isChecked()
    assert window.palette().window().color().lightness() < before
    assert window.store.preferences()["dark"]
    assert not window.debug_toggle.isChecked()
    QTest.mouseClick(window.debug_toggle, Qt.MouseButton.LeftButton)
    assert window.store.preferences()["diagnostics"]
    QTest.mouseClick(window.debug_toggle, Qt.MouseButton.LeftButton)
    assert not window.store.preferences()["diagnostics"]

def test_work_contains_full_width_logs_and_restart(window):
    window.controller.active = True
    window.on_state(dict(screen="Нет сит",next_action="Обновить",evidence="Нет доступных сит",mode="waiting",paused=False,checks=4,remaining_seconds=42,
        journal=[dict(time="2026-10-11T20:35:24.015+02:00",event="flow_restart",status="new_pass",stage="province",page="https://example.test/index",message="Новая сессия")],availability=[]))
    assert window.restart_button.isEnabled()
    assert window.timeline.rowCount() == 1
    assert window.timeline.item(0,0).text() == "20:35:24.015"
    assert window.timeline.item(0,3).toolTip() == window.t("Новая сессия")
    assert not window.profile_combo.isEnabled()


def test_settings_text_uses_available_width_and_theme_is_always_visible(window):
    assert window.dark_toggle.isVisible()
    window.tabs.setCurrentIndex(2)
    for width in (820, 1120):
        window.resize(width, 850); QApplication.processEvents()
        title = window.findChild(__import__('PySide6.QtWidgets', fromlist=['QLabel']).QLabel, 'settingTitle_diagnostics')
        assert title is not None
        assert title.width() > 400
        assert title.height() < 40
        assert window.dark_toggle.isVisible()
    assert not window.windowIcon().isNull()


def test_profile_copy_mode_disables_identity_and_survives_edit(window):
    window.create_profile()
    fields=window.fields
    fields['fill_mode'].setCurrentIndex(fields['fill_mode'].findData('copy'))
    assert not fields['document_number'].isEnabled()
    assert not fields['full_name'].isEnabled()
    for key,value in dict(label='Демонстрация',phone='600000000',email='demo@example.com').items():
        fields[key].setText(value)
    fields['nationality'].setCurrentText('UCRANIA')
    window.save_profile()
    assert window.start_button.isEnabled()
    window.open_editor(window.profile_combo.currentData())
    assert fields['fill_mode'].currentData()=='copy'
    fields['fill_mode'].setCurrentIndex(fields['fill_mode'].findData('manual'))
    assert fields['document_number'].isEnabled() and fields['full_name'].isEnabled()


def test_language_switch_preserves_profile_edits_and_updates_live_status(window):
    window.create_profile()
    window.fields['label'].setText('Личный профиль')
    window.language_actions['en'].trigger()
    QApplication.processEvents()
    assert window.tabs.tabText(0)=='Monitor'
    assert window.fields['label'].text()=='Личный профиль'
    assert window.fields['fill_mode'].itemText(0)=='Enter profile details'
    assert window.store.preferences()['language']=='en'
    window.on_state(dict(screen='Выбор офиса',evidence='Виден селектор доступных офисов',
        next_action='Проверить доступные офисы',mode='working',checks=7))
    assert window.heading.text()=='Office selection'
    assert window.metrics.text().startswith('Checks: 7')
    window.language_actions['es'].trigger()
    assert window.tabs.tabText(0)=='Seguimiento'
    assert window.heading.text()=='Selección de oficina'
    assert window.fields['label'].text()=='Личный профиль'
    assert window.windowTitle()=='Cita Extranjeria Helper'


def test_profile_timing_editor_saves_reload_jitter_and_retry_schedule(window):
    window.create_profile()
    f=window.fields
    f['fill_mode'].setCurrentIndex(f['fill_mode'].findData('copy'))
    for key,value in dict(label='Demo',phone='600000000',email='demo@example.com').items(): f[key].setText(value)
    f['nationality'].setCurrentText('UCRANIA')
    assert window.profile_tabs.tabText(1)=='Timing and retries'
    widgets=window.timing_widgets
    widgets['reload_seconds'][0].setValue(40); widgets['reload_seconds'][1].setValue(55)
    widgets['recovery_jitter_seconds'][1].setValue(7)
    widgets['expired_seconds'][0].setText('0, 45, 90')
    window.save_profile()
    settings=window.store.settings(window.profile_combo.currentData())
    assert settings.assist_timings.reload_seconds==(40,55)
    assert settings.assist_timings.recovery_jitter_seconds==(0,7)
    assert settings.assist_timings.expired_seconds==(0,45,90)
    window.open_editor(window.profile_combo.currentData())
    assert widgets['expired_seconds'][0].text()=='0, 45, 90'


def test_about_includes_version_repository_and_localized_description(window):
    from app.version import VERSION
    from PySide6.QtWidgets import QLabel
    window.about_button.click()
    dialog=window.about_dialog
    assert dialog.isVisible()
    texts=' '.join(w.text() for w in dialog.findChildren(QLabel))
    assert VERSION in texts
    assert window.about_dialog.findChild(__import__('PySide6.QtWidgets',fromlist=['QPushButton']).QPushButton,'githubLink').toolTip() == 'https://github.com/itwormz/extranjeria-cita-helper'
    assert 'appointment' in texts.lower()
    dialog.close()
    window.set_language('es');window.about_button.click()
    assert 'citas' in ' '.join(w.text() for w in window.about_dialog.findChildren(QLabel)).lower()
    window.about_dialog.close()


def test_province_selection_roundtrips_without_barcelona_restriction(window):
    window.create_profile();f=window.fields
    assert f['province'].count()==52
    f['province'].setCurrentText('Madrid')
    f['fill_mode'].setCurrentIndex(f['fill_mode'].findData('copy'))
    for key,value in dict(label='Demo',phone='600000000',email='demo@example.com').items():f[key].setText(value)
    f['nationality'].setCurrentText('UCRANIA')
    window.save_profile()
    assert window.profile_combo.count()==1
    task=window.store.load()[0]
    assert task.province=='Madrid' and not task.errors
    window.open_editor(task.id)
    assert f['province'].currentText()=='Madrid'


def test_profile_details_do_not_stretch_to_timing_page_height(window):
    window.resize(1000,1100);window.create_profile();QApplication.processEvents()
    window.profile_tabs.setCurrentIndex(0);QApplication.processEvents()
    details_height=window.profile_tabs.height()
    window.profile_tabs.setCurrentIndex(1);QApplication.processEvents()
    assert window.profile_tabs.height()==details_height
    window.profile_tabs.setCurrentIndex(0);QApplication.processEvents()
    assert window.fields['phone'].mapTo(window.profile_editor,window.fields['phone'].rect().topLeft()).y()-window.fields['nationality'].mapTo(window.profile_editor,window.fields['nationality'].rect().topLeft()).y()<90


def test_about_has_github_icon_button(window):
    window.about_button.click()
    button=window.about_dialog.findChild(__import__('PySide6.QtWidgets',fromlist=['QPushButton']).QPushButton,'githubLink')
    assert button is not None and not button.icon().isNull()
    window.about_dialog.close()


def test_window_icon_provides_native_raster_sizes(window):
    sizes = {size.width() for size in window.windowIcon().availableSizes()}
    assert {16, 32, 48, 256}.issubset(sizes)
    assert not window.windowIcon().pixmap(32, 32).isNull()


def test_selected_profile_owns_persisted_total(window):
    from app.desktop.statistics import ProfileStatistics
    window.profile_combo.addItem('First', 'first-profile')
    first = window.profile_combo.currentData()
    stats = ProfileStatistics(window.store.path.parent / 'data' / 'profile-statistics.json')
    stats.increment(first)
    stats.increment(first)
    window.profile_changed()
    assert window.metrics.text().endswith('2')
    window.profile_combo.addItem('Second test profile', 'second')
    window.profile_combo.setCurrentIndex(window.profile_combo.count()-1)
    assert window.metrics.text().endswith('0')
    window.profile_combo.setCurrentIndex(window.profile_combo.findData(first))
    assert window.metrics.text().endswith('2')


def test_worker_error_survives_normal_state_heartbeat(window):
    window.show_error('Worker stopped unexpectedly')
    window.on_state(dict(mode='waiting', screen='Нет сит', remaining_seconds=42))
    assert window.error_band.isVisible()
    assert window.error_text.text() == 'Worker stopped unexpectedly'
    window.clear_error('worker')
    assert not window.error_band.isVisible()


def test_manual_state_promotes_browser_and_hides_timer(window):
    window.controller.active = True
    window.on_state(dict(mode='manual', screen='Выбор офиса', remaining_seconds=3))
    assert window.browser_button.property('primary') is True
    assert window.start_button.property('primary') is False
    assert window.timer_label.text()=="—"
    assert window.status_card.property('tone') == 'attention'


def test_profile_dialog_scrolls_and_escape_cancels_without_saving(window):
    from PySide6.QtWidgets import QDialog, QScrollArea
    window.create_profile()
    assert isinstance(window.profile_editor, QDialog)
    assert window.profile_editor.isModal()
    window.profile_editor.resize(680, 500)
    window.profile_tabs.setCurrentIndex(1)
    QApplication.processEvents()
    assert window.profile_tabs.currentWidget().findChild(QScrollArea) is not None or isinstance(window.profile_tabs.currentWidget(), QScrollArea)
    QTest.keyClick(window.profile_editor, Qt.Key.Key_Escape)
    assert not window.profile_editor.isVisible()
    assert window.profile_combo.count() == 0


def test_log_preserves_site_data_and_marks_event_semantics(window):
    window.on_state(dict(mode='working', journal=[dict(time='2026-10-12T12:00:00',event='visit',status='recovering',stage='office',page='https://example.test/Нет сит',message='Доступен офис: {offices}',offices=['Нет сит — CNP GRANOLLERS'])]))
    assert window.timeline.item(0,3).text().endswith('Нет сит — CNP GRANOLLERS')
    assert window.timeline.item(0,1).toolTip().endswith('/Нет сит')
    assert not window.timeline.item(0,2).icon().isNull()
    assert window.timeline.item(0,2).background().color().name() == __import__('app.desktop.theme',fromlist=['theme_colors']).theme_colors(window.preferences['dark'])['attention_soft'].lower()


@pytest.mark.parametrize('locale, expected', [('ru_RU','ru'),('es_ES','es'),('en_US','en'),('de_DE','en')])
def test_first_launch_language_uses_system_locale(tmp_path, catalog_path, monkeypatch, locale, expected):
    import app.desktop.profiles as profiles
    from PySide6.QtCore import QLocale
    monkeypatch.setattr(QLocale, 'system', lambda: QLocale(locale))
    store=ProfileStore(tmp_path/'profiles.yml',catalog_path)
    assert store.preferences()['language'] == expected
    store.save_preferences(dark=False,diagnostics=False,sound=True,language='es')
    monkeypatch.setattr(QLocale, 'system', lambda: QLocale('ru_RU'))
    assert store.preferences()['language'] == 'es'


def test_visible_error_retranslates_without_new_worker_state(window):
    window.show_error('Не удалось сохранить настройки. Изменения не применены.')
    assert window.error_text.text().startswith('Could not') or window.error_text.text().startswith('Failed')
    window.set_language('ru')
    assert window.error_text.text() == 'Не удалось сохранить настройки. Изменения не применены.'
    window.set_language('es')
    assert 'Не удалось' not in window.error_text.text()
    assert window.error_band.isVisible()


def test_office_count_autoselects_once_and_preserves_user_tab(window):
    state=dict(mode='manual',availability=[dict(time='2026-10-12T17:00:00',offices=[{'text':'CNP A'},{'text':'CNP B'}])])
    window.on_state(state)
    assert window.log_tabs.tabText(1).endswith(' · 2')
    assert window.log_tabs.currentIndex()==1
    window.log_tabs.setCurrentIndex(0)
    window.on_state(dict(state,checks=2))
    assert window.log_tabs.currentIndex()==0


def test_fresh_window_is_ready_and_failed_completion_is_danger(window):
    assert window.status_label.text() == 'Ready to start'
    assert window.heading.text() == 'Browser not open yet'
    window.on_finished(1)
    assert window.status_card.property('tone') == 'danger'


def test_controls_keep_geometry_across_states(window):
    window.controller.active=True
    geometries=[]
    for mode in ('working','waiting','manual','recovery','paused'):
        window.on_state(dict(mode=mode,paused=mode=='paused',remaining_seconds=12))
        QApplication.processEvents()
        geometries.append([(b.x(),b.width(),b.height()) for b in (window.start_button,window.browser_button)])
    assert all(g==geometries[0] for g in geometries)
    assert window.log_tabs.cornerWidget(Qt.Corner.TopLeftCorner) is None


def test_about_has_current_latest_and_check_button(window):
    from PySide6.QtWidgets import QLabel,QPushButton
    from app.version import VERSION
    window.show_about()
    dialog=window.about_dialog
    assert dialog.findChild(QLabel,'latestVersion') is not None
    assert dialog.findChild(QPushButton,'checkUpdates').isEnabled()
    assert any(VERSION in label.text() for label in dialog.findChildren(QLabel))
    dialog.close()


@pytest.mark.parametrize('error,expected',[(False,'0.2.0'),(True,'No release has been published')])
def test_about_update_check_handles_release_and_failures(window,monkeypatch,error,expected):
    import io,json
    from urllib.error import HTTPError
    from PySide6.QtWidgets import QLabel,QPushButton
    from app.desktop import updates
    target,arch=updates.release_target()
    extension={'macos':'dmg','windows':'msi','linux':'AppImage'}[target]
    name=f'extranjeria-cita-helper-0.2.0-{target}-{arch}.{extension}'
    base=updates.REPOSITORY+'/releases/download/v0.2.0/'
    payload={'tag_name':'v0.2.0','immutable':True,'assets':[{'name':name,'browser_download_url':base+name,'size':7},
                                         {'name':'SHA256SUMS','browser_download_url':base+'SHA256SUMS'},
                                         {'name':name+'.sigstore.json','browser_download_url':base+name+'.sigstore.json'}]}
    def fetch(url):
        assert url==updates.API
        if error:raise HTTPError(url,404,'missing',{},None)
        return io.BytesIO(json.dumps(payload).encode())
    monkeypatch.setattr(updates,'fetch',fetch)
    window.set_language('en');window.show_about()
    dialog=window.about_dialog;button=dialog.findChild(QPushButton,'checkUpdates')
    button.click();assert not button.isEnabled()
    assert window.update_service.job.wait(1000)
    QApplication.processEvents()
    assert button.isEnabled()
    assert expected in dialog.findChild(QLabel,'latestVersion').text()
    dialog.close()


@pytest.mark.parametrize('retained',[False,True])
def test_window_close_requests_session_shutdown(window,retained):
    window.controller.active=True
    window.state['browser_retained']=retained
    window.close()
    assert window.closing
    assert window.controller.close_browser
    window.controller.finished.emit(0)
    QApplication.processEvents()
    assert not window.isVisible()


def test_both_logs_use_same_readable_time_font(window):
    window.on_state(dict(mode='waiting',journal=[dict(time='2026-10-12T18:00:00.123',event='visit',status='processing',stage='province',page='https://example.test',message='test')],availability=[dict(time='2026-10-12T18:00:00.123',offices=[dict(text='TEST OFFICE')])]))
    from PySide6.QtGui import QFont
    a=window.timeline.item(0,0).font();b=window.offices.item(0,0).font()
    assert a==b
    assert a.family()=='IBM Plex Mono'
    assert a.pixelSize()==14
    assert a.weight()==QFont.Weight.Medium


def test_closed_province_list_passes_wheel_to_profile_scroll(window):
    from PySide6.QtCore import QPoint,QPointF
    from PySide6.QtGui import QWheelEvent
    window.create_profile()
    combo=window.fields['province'];combo.setCurrentIndex(5);combo.setFocus()
    event=QWheelEvent(QPointF(5,5),QPointF(combo.mapToGlobal(QPoint(5,5))),QPoint(),QPoint(0,-120),Qt.MouseButton.NoButton,Qt.KeyboardModifier.NoModifier,Qt.ScrollPhase.NoScrollPhase,False)
    combo.wheelEvent(event)
    assert combo.currentIndex()==5
    assert not event.isAccepted()


def test_profile_values_follow_text_and_numeric_alignment(window):
    from PySide6.QtWidgets import QDoubleSpinBox
    window.create_profile()
    for key in ('full_name','document_number','phone','email'):
        assert window.fields[key].alignment() & Qt.AlignmentFlag.AlignLeft
    for key,widgets in window.timing_widgets.items():
        for widget in widgets:
            if isinstance(widget,QDoubleSpinBox):
                assert widget.alignment() & (Qt.AlignmentFlag.AlignHCenter if len(widgets)==2 else Qt.AlignmentFlag.AlignLeft)
                assert widget.width()>=widget.fontMetrics().horizontalAdvance('3600.00')


@pytest.mark.parametrize('language,filename',[('en','README.md'),('ru','README.ru.md'),('es','README.es.md')])
def test_about_links_to_readme_in_interface_language(window,language,filename):
    from PySide6.QtWidgets import QPushButton
    window.set_language(language);window.show_about()
    link=window.about_dialog.findChild(QPushButton,'readmeLink')
    assert link is not None
    assert link.property('targetUrl').endswith('/'+filename)
    window.about_dialog.close()


def test_about_description_is_not_clipped(window):
    from PySide6.QtWidgets import QLabel
    window.set_language('ru');window.show_about();QApplication.processEvents()
    for text in window.about_dialog.findChildren(QLabel):
        if text.wordWrap():assert text.height()>=text.heightForWidth(text.width())
    window.about_dialog.close()


def test_work_uses_pause_resume_icons_without_stop(window):
    assert not hasattr(window,'stop_button')
    window.controller.active=True
    window.on_state(dict(mode='working',paused=False))
    assert not window.start_button.icon().isNull()
    pause=window.start_button.icon().cacheKey()
    window.on_state(dict(mode='paused',paused=True))
    assert not window.start_button.icon().isNull()
    assert window.start_button.icon().cacheKey()!=pause


def test_toolbar_icons_use_selected_theme_before_palette_polish(window):
    from PySide6.QtGui import QPalette,QColor
    from app.desktop.theme import theme_colors
    window.preferences['dark']=False
    stale=QPalette(window.palette());stale.setColor(QPalette.ColorRole.WindowText,QColor('white'));window.setPalette(stale)
    image=window.control_icon('info').pixmap(18,18).toImage()
    expected=QColor(theme_colors(False)['text'])
    assert any(image.pixelColor(x,y).alpha()>200 and image.pixelColor(x,y).rgb()==expected.rgb() for x in range(image.width()) for y in range(image.height()))


def test_all_lists_and_popup_views_use_themed_focus(window):
    from app.desktop.window import ThemedItemDelegate
    from PySide6.QtWidgets import QComboBox,QFrame
    for combo in window.findChildren(QComboBox):
        assert isinstance(combo.view().itemDelegate(),ThemedItemDelegate)
        assert combo.view().parentWidget().frameShape()==QFrame.Shape.NoFrame
    assert window.settings_profile_combo.maximumWidth()==280
    assert isinstance(window.timeline.itemDelegate(),ThemedItemDelegate)


def test_easter_egg_needs_five_version_clicks_and_is_catchable(window):
    from app.desktop.easter_egg import VersionLabel
    from PySide6.QtCore import QEvent
    window.show_about();version=window.about_dialog.findChild(VersionLabel,'aboutVersion')
    for _ in range(4):QTest.mouseClick(version,Qt.MouseButton.LeftButton)
    assert not hasattr(window,'appointment_chase')
    QTest.mouseClick(version,Qt.MouseButton.LeftButton)
    game=window.appointment_chase
    assert game.width()>=700 and game.height()>=450
    assert game.ticket.text()=='' and not game.ticket.icon().isNull()
    for _ in range(20):game.evade(game.ticket.geometry().center())
    assert game.moves==20
    assert game.ticket.geometry().right()<game.width()
    assert game.ticket.geometry().bottom()<game.height()
    game.ticket.click()
    assert game.caught and not game.ticket.isEnabled()
    assert game.message.text()==window.t('Сита поймана')
    game.close();window.about_dialog.close()


def test_sound_toolbar_mirrors_settings(window):
    window.sound_button.click()
    assert not window.sound_toggle.isChecked()
    assert window.store.preferences()["sound"] is False
    window.sound_toggle.setChecked(True)
    assert window.sound_button.isChecked()

def test_authentication_is_one_three_option_control(window):
    window.open_editor(None)
    assert "clave_method" not in window.fields
    access=window.fields["access_mode"]
    assert [access.itemData(i) for i in range(access.count())]==["certificate","clave_movil","clave_permanente"]


def test_chase_celebration_has_localized_proverb(window):
    from app.desktop.easter_egg import AppointmentChase, PROVERBS
    for language in ("en","ru","es"):
        game=AppointmentChase(window,window.t,language=language)
        game.catch()
        assert game.proverb.text() in PROVERBS[language]
        assert game.celebration_timer.isActive()
        game.close()
        assert not game.celebration_timer.isActive()


def test_idle_reset_only_resets_selected_browser_profile(window,task_mapping,monkeypatch):
    from unittest.mock import Mock
    import copy
    profile=window.store.save_profile(None,dict(copy.deepcopy(task_mapping['tasks'][0]),request=dict(task_mapping['tasks'][0]['request'],access_mode='certificate'),applicant=dict(task_mapping['tasks'][0]['applicant'],contact=dict(phone='600000000',email='demo@example.com'))),revision=window.store.revision())
    window.refresh_profiles(profile)
    reset=Mock();monkeypatch.setattr('app.site.reset_browser_profile',reset)
    window.controller.start=Mock()
    assert window.restart_action.isEnabled()
    window.restart_action.trigger()
    reset.assert_called_once()
    assert reset.call_args.args[1]==profile
    window.controller.start.assert_not_called()
    assert window.store.load()[0].id==profile


@pytest.mark.parametrize('dark',[False,True])
def test_timing_rows_do_not_overlap_at_small_editor_height(window,dark):
    from PySide6.QtCore import QPoint
    window.dark_toggle.setChecked(dark)
    window.create_profile();window.profile_tabs.setCurrentIndex(1)
    window.profile_editor.resize(660,520);QApplication.processEvents()
    scroll=window.profile_tabs.widget(1);page=scroll.widget()
    boxes=list(window.timing_field_boxes.values())
    for index,box in enumerate(boxes):
        assert box.height()<=50
        for other in boxes[index+1:]:assert not box.geometry().intersects(other.geometry())
        for widget in window.timing_widgets[next(key for key,value in window.timing_field_boxes.items() if value is box)]:
            assert box.rect().contains(widget.mapTo(box,QPoint(0,0)))
            assert box.rect().contains(widget.mapTo(box,widget.rect().bottomRight()))


@pytest.mark.parametrize("enabled",[False,True])
def test_bell_changes_only_sound_and_keeps_footer(window,enabled):
    window.debug_toggle.setChecked(enabled)
    before=window.footer.text()
    diagnostics=window.store.preferences()["diagnostics"]
    window.sound_button.click()
    assert window.store.preferences()["diagnostics"]==diagnostics
    assert window.footer.text()==before


@pytest.mark.parametrize("choice,mode,method",[("certificate","certificate","movil"),("clave_movil","clave","movil"),("clave_permanente","clave","permanente")])
def test_unified_authentication_roundtrips(window,choice,mode,method):
    window.create_profile()
    for key,value in dict(label="Demo",full_name="NOMBRE APELLIDO",document_number="X0000000T",phone="600000000",email="demo@example.com").items():
        window.fields[key].setText(value)
    window.fields["nationality"].setCurrentText("UCRANIA")
    access=window.fields["access_mode"];access.setCurrentIndex(access.findData(choice))
    window.save_profile()
    profile=window.store.load()[0]
    assert (profile.access_mode,profile.clave_method)==(mode,method)
    window.open_editor(profile.id)
    assert access.currentData()==choice


@pytest.mark.parametrize('dark',[False,True])
def test_timing_text_area_fits_value_without_visible_arrows(window,dark):
    from PySide6.QtWidgets import QDoubleSpinBox,QStyle,QStyleOptionSpinBox
    window.dark_toggle.setChecked(dark);window.create_profile();window.profile_tabs.setCurrentIndex(1)
    QApplication.processEvents()
    for widgets in window.timing_widgets.values():
        for widget in widgets:
            if not isinstance(widget,QDoubleSpinBox):continue
            editor=widget.lineEdit()
            assert editor.width()>=editor.fontMetrics().horizontalAdvance(widget.text())+8
            assert editor.height()>=editor.fontMetrics().height()
            option=QStyleOptionSpinBox();widget.initStyleOption(option)
            for control in (QStyle.SubControl.SC_SpinBoxUp,QStyle.SubControl.SC_SpinBoxDown):
                rect=widget.style().subControlRect(QStyle.ComplexControl.CC_SpinBox,option,control,widget)
                assert rect.isEmpty()


def test_easter_egg_has_fifteen_unique_proverbs_per_language():
    from app.desktop.easter_egg import PROVERBS
    assert set(PROVERBS)=={'ru','en','es'}
    for phrases in PROVERBS.values():
        assert len(phrases)==len(set(phrases))==15
        assert all(phrase.strip() for phrase in phrases)


def test_timeline_uses_visits_and_ignores_raw_diagnostics(window):
    from app.assist_control import AssistControl
    control=AssistControl()
    control.observe_visit(document='one',stage='applicant',page='https://site.test/form',title='form')
    for event in ['capture','response','action','action_completed']:
        control.record(event,document='one',stage='applicant',detail=event)
    window.on_state(control.snapshot())
    assert window.timeline.rowCount()==1
    assert window.timeline.item(0,2).text()==window.t('Обработка')
    assert window.timeline.horizontalHeaderItem(2).text()==window.t('Статус')
    control.observe_visit(document='two',stage='request_menu',page='https://site.test/form',title='menu')
    window.on_state(control.snapshot())
    assert window.timeline.rowCount()==2
    assert window.timeline.item(1,2).text()==window.t('Пройдено')
    window.copy_log()
    copied=QApplication.clipboard().text()
    assert 'capture' not in copied and 'action_completed' not in copied


def test_state_card_labels_and_automatic_participation(window):
    window.preferences["language"] = "ru"
    window.controller.active = True
    window.on_state(dict(screen="Сит нет — требуется новый проход", evidence="salirInicio", next_action="Подождать перед новым проходом — затем Aceptar", mode="waiting", remaining_seconds=6))
    assert window.heading.text() == "Сит нет"
    assert window.page_caption.text() == "Страница"
    assert window.bot_caption.text() == "Бот"
    assert window.action_caption.text() == "Далее"
    assert window.attention_label.text() == "Участие не требуется"
    assert window.timer_caption.text() == "до нового прохода"
    assert window.evidence.isHidden()


def test_finished_card_keeps_last_site_page_and_clears_old_action(window):
    window.preferences['language']='ru'
    window.on_state(dict(screen='Выбор офиса',next_action='Выбрать офис',mode='working'))
    window.on_finished(0)
    assert window.heading.text()=='Выбор офиса'
    assert window.next_label.text()=='Для начала — «Запустить»'


@pytest.mark.parametrize('dark',[False,True])
def test_manual_participation_is_attention_color(window,dark):
    from PySide6.QtGui import QPalette,QColor
    from app.desktop.theme import theme_colors
    window.dark_toggle.setChecked(dark)
    window.controller.active=True
    window.on_state(dict(screen='Выбор офиса',next_action='Выбери офис',mode='manual'))
    assert window.attention_label.palette().color(QPalette.ColorRole.WindowText)==QColor(theme_colors(dark)['attention'])


def test_status_summary_is_compact_and_controls_stay_in_toolbar(window):
    window.controller.active=True
    window.on_state(dict(screen='Сит нет',next_action='Подождать перед новым проходом — затем Aceptar',mode='waiting',remaining_seconds=6))
    QApplication.processEvents()
    assert window.status_card.height()<160
    assert window.attention_label.isVisible()
    assert window.heading.mapTo(window.status_card,window.heading.rect().topLeft()).y()<window.status_label.mapTo(window.status_card,window.status_label.rect().topLeft()).y()
    assert window.heading.mapTo(window.status_card,window.heading.rect().topLeft()).x()==window.status_label.mapTo(window.status_card,window.status_label.rect().topLeft()).x()
    assert window.start_button.parentWidget() is window.control_toolbar


def test_update_toggle_and_about_download_are_available(window):
    from PySide6.QtWidgets import QPushButton
    assert window.updates_toggle.isChecked()
    window.updates_toggle.setChecked(False)
    assert window.store.preferences()['automatic_updates'] is False
    window.show_about()
    dialog=window.about_dialog
    assert dialog.findChild(QPushButton,'checkUpdates') is not None
    assert dialog.findChild(QPushButton,'downloadUpdate') is not None
    assert dialog.findChild(QPushButton,'installUpdate') is not None


def test_install_waits_for_active_browser_to_stop(window,tmp_path,monkeypatch):
    package=tmp_path/'package.msi';package.write_bytes(b'package')
    window.controller.active=True
    window.update_install_requested=True
    launched=[]
    monkeypatch.setattr(window,'launch_update',lambda path:launched.append(path))
    window.update_verified(package)
    assert window.controller.close_browser is True
    assert not launched
    window.on_finished(0)
    assert launched==[package]


def test_new_release_does_not_offer_previously_downloaded_package(window,tmp_path):
    from types import SimpleNamespace
    window.update_path=tmp_path/'old-package.msi'
    window.update_checked(SimpleNamespace(version='0.3.0',name='new-package.msi'))
    assert window.update_path is None


def test_copy_log_uses_selected_rows_in_visual_order(window):
    from PySide6.QtWidgets import QTableWidgetItem
    from PySide6.QtCore import QItemSelectionModel
    table=window.timeline
    table.setRowCount(3)
    for row in range(3):
        for col in range(4):table.setItem(row,col,QTableWidgetItem(f"row{row}-col{col}"))
    flags=QItemSelectionModel.SelectionFlag.Select|QItemSelectionModel.SelectionFlag.Rows
    table.selectionModel().select(table.model().index(2,0),flags)
    table.selectionModel().select(table.model().index(0,0),flags)
    window.copy_log()
    lines=QApplication.clipboard().text().splitlines()
    assert len(lines)==3
    assert lines[1].startswith("row0-col0") and lines[2].startswith("row2-col0")
    assert "row1-col0" not in QApplication.clipboard().text()


def test_profile_fields_follow_website_order(window):
    window.create_profile();QApplication.processEvents()
    grid=window.fields['label'].parentWidget().parentWidget().layout()
    keys=['label','province','access_mode','document_number','full_name','nationality','phone','email']
    positions=[grid.getItemPosition(grid.indexOf(window.fields[key].parentWidget()))[:2] for key in keys]
    assert positions==sorted(positions)
    assert len(set(positions))==len(keys)


def test_identity_source_hides_inputs_and_keeps_manual_draft(window):
    window.create_profile();f=window.fields
    f['document_number'].setText('X0000000T');f['full_name'].setText('DEMO PERSON')
    f['fill_mode'].setCurrentIndex(f['fill_mode'].findData('copy'))
    assert f['document_number'].parentWidget().isHidden()
    assert f['full_name'].parentWidget().isHidden()
    assert not window.identity_note.isHidden()
    f['fill_mode'].setCurrentIndex(f['fill_mode'].findData('manual'))
    assert not f['document_number'].parentWidget().isHidden()
    assert not f['full_name'].parentWidget().isHidden()
    assert window.identity_note.isHidden()
    assert f['document_number'].text()=='X0000000T' and f['full_name'].text()=='DEMO PERSON'


def test_booking_handoff_disables_automation_control(window):
    window.controller.active=True
    window.set_language('en')
    window.on_state(dict(screen='Выбор ситы',mode='manual',booking_handoff=True,next_action='Выбери дату и время в окне сайта'))
    assert not window.start_button.isEnabled()
    assert window.start_button.text()=='Manual booking'
    assert window.status_label.text()=='Control handed over'
    assert window.browser_button.isEnabled()
    window.controller.active=False


def test_application_icon_tracks_search_pause_attention_and_failure(window):
    assert window.icon_status == 'idle'
    window.controller.active=True
    for state,status in [({'mode':'working'},'running'),
                         ({'mode':'waiting'},'running'),
                         ({'mode':'recovery'},'running'),
                         ({'mode':'paused','paused':True},'paused'),
                         ({'mode':'manual'},'attention'),
                         ({'mode':'manual','booking_handoff':True},'attention'),
                         ({'mode':'working','connection_lost':True},'error')]:
        window.on_state(state)
        assert window.icon_status == status
        assert not window.windowIcon().isNull()
    window.on_state({'mode':'working'})
    window.show_error('Example','update')
    assert window.icon_status == 'running'  # Update errors must not stop the search indicator.
    window.show_error('Example','worker')
    assert window.icon_status == 'error'
    window.clear_error('worker')
    assert window.icon_status == 'running'
    window.controller.active=False
    window.on_finished(0)
    assert window.icon_status == 'idle'


def test_status_icons_have_distinct_indicator_pixels(qt_app):
    from app.desktop.status_icon import application_icon
    images={name:application_icon(name).pixmap(64,64).toImage() for name in ('idle','running','paused','attention','error')}
    assert len({image.pixelColor(54,54).name() for image in images.values()}) == 5


def test_work_summary_columns_do_not_move_between_states(window):
    window.controller.active=True
    positions=[]
    for mode,screen,action,remaining in [('working','Выбор провинции','Выбрать провинцию',None),('waiting','Сит нет — новый проход','Aceptar → начать заново',171),('manual','Выбор даты и времени','Оформить запись в Camoufox',None)]:
        window.on_state(dict(mode=mode,screen=screen,next_action=action,remaining_seconds=remaining))
        QApplication.processEvents()
        positions.append((window.heading.pos().x(),window.timer_label.parentWidget().pos().x(),window.status_card.height()))
    assert len(set(positions))==1


def test_log_selection_clears_on_empty_space_and_escape(window):
    from PySide6.QtWidgets import QTableWidgetItem
    from PySide6.QtCore import QPoint
    table=window.timeline;table.setRowCount(1);table.setItem(0,0,QTableWidgetItem('10:00'))
    table.selectRow(0)
    QTest.mouseClick(table.viewport(),Qt.MouseButton.LeftButton,pos=QPoint(10,table.viewport().height()-10))
    assert not table.selectedItems()
    table.selectRow(0);table.setFocus();QTest.keyClick(table,Qt.Key.Key_Escape)
    assert not table.selectedItems()
    assert not table.currentIndex().isValid()


def test_work_status_has_no_scroll_container_and_copy_is_icon_only(window):
    from PySide6.QtWidgets import QScrollArea
    parent=window.status_card.parentWidget()
    while parent is not None:
        assert not isinstance(parent,QScrollArea)
        parent=parent.parentWidget()
    assert window.copy_button.text()==''
    assert not window.copy_button.icon().isNull()
    assert not window.browser_button.icon().isNull()


def test_controls_below_status_allow_640_pixel_window(window):
    window.resize(640,780);window.controller.active=True
    window.on_state(dict(mode='waiting',screen='Сит нет — новый проход',next_action='Aceptar → начать заново',remaining_seconds=171))
    QApplication.processEvents()
    assert window.width()==640
    def point(widget):return widget.mapTo(window,widget.rect().topLeft())
    assert point(window.start_button).y()>point(window.next_label).y()
    assert point(window.metrics).y()>point(window.start_button).y()
    assert point(window.more_actions).x()+window.more_actions.width()<=window.width()-16
    assert window.attention_label.isVisible()


def test_monitor_scrolls_log_horizontally_and_has_timer_separator(window):
    window.resize(640,780);QApplication.processEvents()
    assert window.timeline.horizontalScrollBar().maximum()>0
    assert window.findChild(__import__('PySide6.QtWidgets',fromlist=['QWidget']).QWidget,'timerColumn') is not None
    assert window.metrics.font().pixelSize()==14
    assert window.timeline.verticalHeader().defaultSectionSize()==40


def test_click_outside_log_clears_selection_except_copy(window):
    from PySide6.QtWidgets import QTableWidgetItem
    table=window.timeline;table.setRowCount(1);table.setItem(0,0,QTableWidgetItem('10:00'))
    table.selectRow(0)
    QTest.mouseClick(window.copy_button,Qt.MouseButton.LeftButton)
    assert table.selectedItems()
    QTest.mouseClick(window.heading,Qt.MouseButton.LeftButton)
    assert not table.selectedItems()
    assert not table.currentIndex().isValid()


def test_empty_journals_hide_table_and_disable_copy_until_rows_arrive(window):
    assert window.timeline.isHidden()
    assert window.timeline_empty.isVisible()
    assert not window.copy_button.isEnabled()
    window.on_state(dict(mode='waiting',journal=[dict(time='2026-10-14T14:43:50.689',event='visit',status='passed',stage='request_menu',page='https://example.test',message='test')]))
    assert not window.timeline.isHidden()
    assert window.timeline_empty.isHidden()
    assert window.copy_button.isEnabled()
    window.log_tabs.setCurrentIndex(1)
    assert window.offices.isHidden()
    assert window.offices_empty.isVisible()
    assert not window.copy_button.isEnabled()
    window.on_state(dict(mode='waiting',availability=[dict(time='2026-10-14T14:45:00.000',offices=[dict(text='TEST OFFICE')])]))
    assert not window.offices.isHidden()
    assert window.offices_empty.isHidden()
    assert window.copy_button.isEnabled()


def test_startup_error_has_copy_report_and_copies_technical_data(window, tmp_path, monkeypatch):
    from app.desktop import startup_report as reports
    monkeypatch.setattr(reports, 'runtime_root', lambda: tmp_path)
    monkeypatch.setattr(reports, 'bundled_browser_candidate', lambda: tmp_path / 'missing-browser')
    monkeypatch.setattr(reports.sys, 'frozen', True, raising=False)
    report_id, summary = reports.save_startup_error(FileNotFoundError('missing browser'), 'browser_launch')
    window.preferences['language'] = 'ru'
    window.show_error(f'Browser startup failed — {summary} — startup-errors/{report_id}.json')
    assert window.copy_report_button.isVisible()
    assert window.error_text.text() == 'Встроенный браузер не найден'
    QTest.mouseClick(window.copy_report_button, Qt.MouseButton.LeftButton)
    copied = QApplication.clipboard().text()
    assert report_id in copied
    assert 'FileNotFoundError' in copied
    assert 'browser_launch' in copied
    assert window.copy_report_button.text() == 'Отчёт скопирован'
    window.clear_error('worker')
    assert not window.copy_report_button.isVisible()


def test_ordinary_error_does_not_offer_unrelated_startup_report(window):
    window.show_error('Example failure')
    assert not window.copy_report_button.isVisible()


def test_access_denied_is_not_presented_as_missing_browser(window, tmp_path, monkeypatch):
    import json
    from app.desktop import startup_report as reports
    monkeypatch.setattr(reports, 'runtime_root', lambda: tmp_path)
    report_id, summary = reports.save_startup_error(PermissionError('denied'), 'browser_launch')
    path=tmp_path/'startup-errors'/(report_id+'.json')
    data=json.loads(path.read_text()); data['frozen']=True
    data['browser']={'exists':False,'stat_error':{'errno':13}}
    path.write_text(json.dumps(data))
    window.preferences['language']='ru'
    window.show_error(f'Browser startup failed — {summary} — startup-errors/{report_id}.json')
    assert window.error_text.text()=='Нет доступа к встроенному браузеру'
    assert window.copy_report_button.isVisible()


def test_disabled_automatic_updates_do_not_make_check(window, monkeypatch):
    calls = []
    window.preferences['automatic_updates'] = False
    monkeypatch.setattr(window, 'check_updates', lambda: calls.append('check'))
    window.automatic_update_check()
    assert calls == []


def test_macos_install_opens_verified_dmg_then_closes_app(window, monkeypatch, tmp_path):
    import subprocess
    import sys
    package = tmp_path / 'update.dmg'
    calls = []
    monkeypatch.setattr(sys, 'platform', 'darwin')
    monkeypatch.setattr(subprocess, 'Popen', lambda args: calls.append(args))
    monkeypatch.setattr(window, 'close', lambda: calls.append('close'))
    window.launch_update(package)
    assert calls == [['/usr/bin/open', str(package)], 'close']


@pytest.mark.parametrize('dark',[False,True])
def test_about_provenance_failure_has_danger_shield_and_clears_on_retry(window,dark):
    from PySide6.QtWidgets import QLabel,QFrame,QPushButton
    window.dark_toggle.setChecked(dark)
    window.show_about()
    window.update_failed('provenance');QApplication.processEvents()
    dialog=window.about_dialog
    assert dialog.findChild(QFrame,'aboutUpdates').property('tone')=='danger'
    assert dialog.findChild(QLabel,'aboutUpdateShield').isVisible()
    status=dialog.findChild(QLabel,'latestVersion')
    assert status.property('tone')=='danger'
    assert status.height()>=status.heightForWidth(status.width())
    assert not dialog.findChild(QPushButton,'installUpdate').isVisible()
    window.update_checked(None);QApplication.processEvents()
    assert dialog.findChild(QFrame,'aboutUpdates').property('tone')=='normal'
    assert not dialog.findChild(QLabel,'aboutUpdateShield').isVisible()
    dialog.close()
