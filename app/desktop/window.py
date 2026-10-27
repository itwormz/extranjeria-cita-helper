"""Native control window: work with journals, and profile/settings editing."""
import sys
from pathlib import Path

from PySide6.QtCore import QEvent, Qt, QUrl, QSize, QByteArray, QRectF, QTimer
from PySide6.QtGui import QAction, QKeySequence, QAccessible, QAccessibleAnnouncementEvent, QColor, QDesktopServices, QFont, QIcon, QPainter, QPixmap, QPen
from PySide6.QtSvg import QSvgRenderer
from PySide6.QtWidgets import (QAbstractButton, QAbstractItemView, QApplication, QComboBox, QDoubleSpinBox,
    QDialog, QDialogButtonBox, QButtonGroup, QGridLayout, QLayout, QHeaderView, QMenu, QToolButton, QHBoxLayout, QLabel, QLineEdit, QListWidget, QListWidgetItem,
    QMainWindow, QPushButton, QScrollArea, QSizePolicy, QTabWidget, QFrame,
    QListView, QStyledItemDelegate, QStyleOptionViewItem, QStyle,
    QTableWidget, QTableWidgetItem, QVBoxLayout, QWidget)

from ..config import TIE_PROCEDURE, REGRESO_PROCEDURE, PROCEDURE_NAMES, _load_catalog, load_dotenv
from ..assist_flow import LABELS
from .profiles import ProfileStore, ProfileConflictError, ProfileValidationError
from .process import DesktopController
from .theme import apply_theme, theme_colors, load_fonts
from .i18n import LANGUAGES, translate
from .paths import runtime_root, display_name
from .profile_widgets import ProfileField, TimingField

from ..assist_journal import STATUSES, description

CONTROL_LABELS = {'start':'Запуск', 'finish':'Завершение', 'pause':'Пауза', 'resume':'Продолжение',
    'flow_start':'Начало прохода', 'flow_restart':'Новый проход', 'handoff':'Ручное управление',
    'statistics_error':'Ошибка счётчика', 'browser_shown':'Camoufox показан',
    'browser_show_failed':'Не удалось показать Camoufox'}


def event_icon(color):
    pixmap=QPixmap(12,12); pixmap.fill(Qt.GlobalColor.transparent)
    painter=QPainter(pixmap); painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    painter.setPen(Qt.PenStyle.NoPen); painter.setBrush(QColor(color)); painter.drawEllipse(3,3,6,6); painter.end()
    return QIcon(pixmap)


def time_font():
    font=QFont('IBM Plex Mono')
    font.setPixelSize(14)
    font.setWeight(QFont.Weight.Medium)
    return font


def label(text='', *, muted=False, name=None):
    widget = QLabel(text)
    widget.setWordWrap(True)
    widget.setTextFormat(Qt.TextFormat.PlainText)
    widget.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)
    if muted:
        widget.setProperty('muted', True)
    if name:
        widget.setObjectName(name)
    return widget


class ThemedItemDelegate(QStyledItemDelegate):
    def paint(self,painter,option,index):
        styled=QStyleOptionViewItem(option)
        focused=bool(styled.state & QStyle.StateFlag.State_HasFocus)
        styled.state &= ~QStyle.StateFlag.State_HasFocus
        if isinstance(self.parent(),QListView):
            painter.save()
            if styled.state & QStyle.StateFlag.State_Selected:
                painter.fillRect(styled.rect,styled.palette.highlight())
            painter.setPen(styled.palette.text().color())
            rect=styled.rect.adjusted(12,0,-12,0)
            painter.drawText(rect,Qt.AlignmentFlag.AlignVCenter|Qt.AlignmentFlag.AlignLeft,styled.fontMetrics.elidedText(str(index.data() or ''),Qt.TextElideMode.ElideRight,rect.width()))
            painter.restore()
            return
        super().paint(painter,styled,index)
        if focused:
            painter.save(); painter.setPen(QPen(QColor('#5E6AD2'),1))
            painter.setBrush(Qt.BrushStyle.NoBrush)
            painter.drawRoundedRect(QRectF(option.rect.adjusted(1,1,-1,-1)),4,4)
            painter.restore()


class JournalTable(QTableWidget):
    def resizeEvent(self,event):
        super().resizeEvent(event)
        if hasattr(self,'detail_minimum'):
            fixed=sum(self.columnWidth(i) for i in range(self.columnCount()-1))
            self.setColumnWidth(self.columnCount()-1,max(self.detail_minimum,self.viewport().width()-fixed))

    def clear_log_selection(self):
        self.clearSelection()
        self.setCurrentCell(-1,-1)

    def mousePressEvent(self,event):
        if event.button()==Qt.MouseButton.LeftButton and not self.indexAt(event.position().toPoint()).isValid():
            self.clear_log_selection()
            event.accept()
            return
        super().mousePressEvent(event)

    def keyPressEvent(self,event):
        if event.key()==Qt.Key.Key_Escape:
            self.clear_log_selection()
            event.accept()
            return
        super().keyPressEvent(event)


class ProfileComboBox(QComboBox):
    def wheelEvent(self, event):
        if self.view().isVisible():
            super().wheelEvent(event)
        else:
            event.ignore()


class Toggle(QAbstractButton):
    def __init__(self, name):
        super().__init__()
        self.setCheckable(True)
        self.setAccessibleName(name)
        self.setFixedSize(42, 25)
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setPen(Qt.PenStyle.NoPen)
        color = self.palette().link().color() if self.isChecked() else self.palette().mid().color()
        painter.setBrush(color)
        painter.drawRoundedRect(1, 2, 40, 22, 11, 11)
        painter.setBrush(self.palette().window().color())
        painter.drawEllipse(22 if self.isChecked() else 3, 4, 18, 18)
        if self.hasFocus():
            painter.setBrush(Qt.BrushStyle.NoBrush)
            painter.setPen(self.palette().link().color())
            painter.drawRoundedRect(0, 0, 41, 24, 12, 12)


class DesktopWindow(QMainWindow):
    def __init__(self, store, controller):
        super().__init__()
        self.store, self.controller = store, controller
        load_fonts(QApplication.instance())
        self.state = {}
        self.errors = {}
        self._availability_count = 0
        self._last_logs = None
        self._translation_sources = {}
        self.closing = False
        self.editing_id = None
        self.editor_revision = ''
        self.setWindowTitle(display_name())
        self._status_icons={}
        self.icon_status='idle'
        self.update_application_icon()
        self.resize(1120, 850)
        self.setMinimumSize(640, 620)
        self.tabs = QTabWidget();self.tabs.tabBar().setObjectName("mainNavigation")
        self.setCentralWidget(self.tabs)
        try:self.preferences = self.store.preferences()
        except ProfileValidationError:self.preferences = dict(dark=False, diagnostics=False, sound=True, language='en', automatic_updates=True)
        self._work()
        self._settings()
        self._statistics()
        self.tabs.currentChanged.connect(self.main_tab_changed)
        theme_bar = QWidget(); theme_layout = QHBoxLayout(theme_bar)
        theme_layout.setContentsMargins(12, 10, 20, 10)
        theme_layout.setSpacing(8)
        self.language_button = QToolButton()
        self.language_button.setPopupMode(QToolButton.ToolButtonPopupMode.InstantPopup)
        language_menu=QMenu(self.language_button)
        self.language_actions={}
        for code,title in LANGUAGES.items():
            action=language_menu.addAction(code.upper()+' '+title)
            action.setCheckable(True); action.triggered.connect(lambda checked,c=code:self.set_language(c))
            self.language_actions[code]=action
        self.language_button.setMenu(language_menu); theme_layout.addWidget(self.language_button)
        self.dark_toggle = QToolButton()
        self.dark_toggle.setCheckable(True)
        self.dark_toggle.setAccessibleName('Тёмная тема')
        theme_layout.addWidget(self.dark_toggle)
        self.sound_button=QToolButton(); self.sound_button.setCheckable(True)
        self.sound_button.toggled.connect(self.sound_toggle.setChecked); theme_layout.addWidget(self.sound_button)
        self.about_button=QToolButton(); self.about_button.clicked.connect(self.show_about); theme_layout.addWidget(self.about_button)
        for button in (self.language_button, self.dark_toggle, self.sound_button, self.about_button):
            button.setProperty("toolbarAction",True)
            button.setFixedSize(36, 36)
            button.setIconSize(QSize(18, 18))
        self.tabs.setCornerWidget(theme_bar, Qt.Corner.TopRightCorner)
        for combo in self.findChildren(QComboBox):
            view=QListView(combo); view.setItemDelegate(ThemedItemDelegate(view)); combo.setView(view)
            popup=view.parentWidget()
            if isinstance(popup,QFrame):
                popup.setFrameShape(QFrame.Shape.NoFrame); popup.setObjectName('comboPopup')
        for view in self.findChildren(QListWidget)+self.findChildren(QTableWidget):
            view.setItemDelegate(ThemedItemDelegate(view))
        from .notification_delivery import NotificationDelivery
        self.notification_delivery=NotificationDelivery(store,self)
        self.notification_delivery.result.connect(self.notification_result)
        controller.state_changed.connect(self.on_state)
        controller.failed.connect(self.show_error)
        controller.finished.connect(self.on_finished)
        try:
            self.preferences = store.preferences()
        except ProfileValidationError as exc:
            self.preferences = dict(dark=False, diagnostics=False, sound=True, language='en', automatic_updates=True)
            self.show_error(str(exc))
        for key, toggle in [('dark',self.dark_toggle),('diagnostics',self.debug_toggle),('sound',self.sound_toggle),('automatic_updates',self.updates_toggle)]:
            toggle.setChecked(self.preferences[key])
            toggle.toggled.connect(self.save_preferences)
        apply_theme(QApplication.instance(), dark=self.preferences['dark'])
        from .updates import UpdateService
        from ..version import VERSION
        self.update_service=UpdateService(store.path.parent/'data/updates',VERSION,self)
        self.update_release=None;self.update_path=None;self.update_message='Ещё не проверена'
        self.update_install_requested=False;self.pending_update_install=None
        self.update_service.checked.connect(self.update_checked)
        self.update_service.downloaded.connect(self.update_downloaded)
        self.update_service.verified.connect(self.update_verified)
        self.update_service.failed.connect(self.update_failed)
        self.update_service.busy_changed.connect(self.update_busy)
        self.update_service.progress.connect(self.update_progress)
        self.refresh_profiles()
        for button in (self.restart_button,self.browser_button): button.setEnabled(False)
        self.update_start(); self.update_state_presentation(); self.retranslate()
        QApplication.instance().installEventFilter(self)

    def _work(self):
        page = QWidget();page.setObjectName("monitorPage"); outer = QVBoxLayout(page); outer.setContentsMargins(24, 20, 24, 18)
        self.tabs.addTab(page, 'Работа')
        profile_row = QHBoxLayout()
        profile_row.addWidget(label('Профиль'))
        self.profile_combo = QComboBox(); self.profile_combo.setMinimumWidth(200);self.profile_combo.setMaximumWidth(280)
        self.profile_combo.currentIndexChanged.connect(self.profile_changed)
        profile_row.addWidget(self.profile_combo); profile_row.addStretch()
        outer.addLayout(profile_row)
        service_row=QHBoxLayout();service_row.setSpacing(8)
        self.service_icon=QLabel();self.service_icon.setFixedSize(18,18);service_row.addWidget(self.service_icon)
        self.service_label=label('',muted=True); self.service_label.setObjectName('profileService')
        service_row.addWidget(self.service_label,1);outer.addLayout(service_row)
        status_page = QWidget(); layout=QVBoxLayout(status_page); layout.setContentsMargins(0,20,0,0); layout.setSpacing(14)
        self.status_card=QWidget(); self.status_card.setObjectName('workStatus')
        self.status_card.setSizePolicy(QSizePolicy.Policy.Expanding,QSizePolicy.Policy.Maximum)
        card=QHBoxLayout(self.status_card); card.setContentsMargins(0,0,0,0); card.setSpacing(0)
        self.state_rail=QWidget(); self.state_rail.setObjectName('stateRail'); self.state_rail.setFixedWidth(3); card.addWidget(self.state_rail)
        content=QVBoxLayout(); content.setContentsMargins(0,0,0,0); content.setSpacing(12); card.addLayout(content,1)
        self.status_label=label('Готов к запуску',name='stateLabel')
        self.status_dot=QLabel(); self.status_dot.setFixedSize(12,12);self.status_dot.hide()

        self.heading=label('Браузер ещё не открыт',name='heading')
        self.evidence=label('Выбрать профиль. Данные можно изменить в настройках.',muted=True)
        self.next_label=label('Открыть Camoufox и начать с выбора провинции',name='nextAction')
        self.page_caption=label('Страница',muted=True)
        self.bot_caption=label('Бот',muted=True)
        self.action_caption=label('Далее',muted=True)
        self.heading.setObjectName('nextAction')
        summary=QGridLayout();summary.setHorizontalSpacing(16);summary.setVerticalSpacing(0)
        summary.setColumnMinimumWidth(0,76);summary.setColumnStretch(1,1)
        for row,(caption,value) in enumerate(((self.page_caption,self.heading),(self.bot_caption,self.status_label),(self.action_caption,self.next_label))):
            caption.setFixedWidth(76);caption.setWordWrap(False)
            value.setFixedHeight(32);value.setToolTip(value.text())
            summary.addWidget(caption,row,0)
            summary.addWidget(value,row,1)
        self.timer_label=label('—',name='compactTimer');self.timer_caption=label('до следующего действия',muted=True)
        timer_box=QWidget();timer_box.setObjectName("timerColumn");timer_box.setFixedWidth(140)
        timer_layout=QVBoxLayout(timer_box);timer_layout.setContentsMargins(16,0,0,0);timer_layout.setSpacing(2)
        timer_layout.addWidget(self.timer_label);timer_layout.addWidget(self.timer_caption);timer_layout.addStretch()
        summary.addWidget(timer_box,0,2,3,1);content.addLayout(summary)
        self.evidence.hide()
        self.attention_label=label('Участие не требуется',muted=True,name='participationLabel')
        self.attention_label.setFixedHeight(24)
        policy=self.attention_label.sizePolicy();policy.setRetainSizeWhenHidden(True);self.attention_label.setSizePolicy(policy)
        content.addWidget(self.attention_label)
        self.control_toolbar=QWidget()
        self.actions=QHBoxLayout(self.control_toolbar);self.actions.setContentsMargins(0,0,0,0);self.actions.setSpacing(8)
        self.start_button=QPushButton('Запустить'); self.start_button.setProperty('primary',True); self.start_button.clicked.connect(self.start_or_pause)
        self.restart_button=QPushButton('Перезапустить сессию'); self.restart_button.clicked.connect(lambda:self.controller.send({'command':'restart'}))
        self.browser_button=QPushButton('Показать Camoufox');self.browser_button.setProperty('toolbarAction',True); self.browser_button.clicked.connect(lambda:self.controller.send({'command':'show_browser'}))
        for button in (self.start_button,self.restart_button,self.browser_button): self.actions.addWidget(button)
        self.more_actions=QToolButton();self.more_actions.setProperty('toolbarAction',True); self.more_actions.setText('⋯'); self.more_actions.setFixedSize(36,36)
        self.more_actions.setPopupMode(QToolButton.ToolButtonPopupMode.InstantPopup)
        menu=QMenu(self.more_actions); self.restart_action=menu.addAction('Перезапустить сессию')
        self.restart_action.triggered.connect(self.reset_session)
        self.success_action=menu.addAction('Сита получена')
        self.success_action.triggered.connect(self.mark_success_from_work)
        self.more_actions.setMenu(menu)
        self.actions.addWidget(self.more_actions); self.actions.addStretch()
        self.error_band=QWidget(); self.error_band.setObjectName('errorBand')
        error_layout=QVBoxLayout(self.error_band); error_layout.setContentsMargins(16,14,16,14); error_layout.setSpacing(10)
        error_heading=QHBoxLayout(); error_heading.setSpacing(10)
        self.error_icon=QLabel(); self.error_icon.setFixedSize(20,20); error_heading.addWidget(self.error_icon)
        self.error_text=label(); error_heading.addWidget(self.error_text,1); error_layout.addLayout(error_heading)
        self.error_help=label('Скопировать отчёт и отправить автору для проверки.')
        error_layout.addWidget(self.error_help)
        report_actions=QHBoxLayout(); report_actions.setSpacing(16)
        self.copy_report_button=QPushButton('Скопировать отчёт'); self.copy_report_button.setProperty('primary',True)
        self.copy_report_button.clicked.connect(self.copy_startup_report)
        report_actions.addWidget(self.copy_report_button)
        self.report_privacy=label('Без данных профиля и содержимого страниц',muted=True)
        report_actions.addWidget(self.report_privacy,1); error_layout.addLayout(report_actions)
        self.report_reference=label(muted=True); error_layout.addWidget(self.report_reference)
        self.startup_report_id=None
        self.report_copy_timer=QTimer(self); self.report_copy_timer.setSingleShot(True)
        self.report_copy_timer.timeout.connect(self.refresh_error_band)
        content.addWidget(self.error_band); self.error_band.hide()
        self.state_rail.hide()
        layout.addWidget(self.status_card)
        layout.addWidget(self.control_toolbar)
        self.metrics=label('Проверки: 0 за запуск · Всего: —',muted=True,name='compactMetrics')
        self.metrics.setWordWrap(False)
        self.notice=label('Бот сообщит, когда потребуется участие.',muted=True);self.notice.hide()
        layout.addWidget(self.metrics)
        status_page.setSizePolicy(QSizePolicy.Policy.Expanding,QSizePolicy.Policy.Maximum)
        outer.addWidget(status_page)
        journals = QWidget(); journals_layout = QVBoxLayout(journals); journals_layout.setContentsMargins(0,8,0,0)
        separator=QFrame();separator.setObjectName("journalSeparator");separator.setFixedHeight(1);journals_layout.addWidget(separator)
        self.copy_button = copy = QToolButton();copy.setFixedSize(36,36);copy.setProperty('toolbarAction',True);copy.clicked.connect(self.copy_log)
        self.log_tabs = QTabWidget();self.log_tabs.setObjectName('journalTabs')
        journal_tools=QWidget();journal_tools.setObjectName("journalTools");tools=QHBoxLayout(journal_tools);tools.setContentsMargins(0,8,0,8);tools.setSpacing(12)
        tools.addWidget(copy)
        self.log_tabs.setCornerWidget(journal_tools,Qt.Corner.TopRightCorner)
        self.timeline = self.table(['Время','Страница','Статус','Что произошло'])
        self.offices = self.table(['Время','Доступные офисы'])
        self.empty_icon_labels=[]
        self.timeline_empty=self.empty_journal('Записей пока нет','list')
        self.offices_empty=self.empty_journal('Офисы пока не появлялись','office')
        timeline_page=QWidget(); timeline_layout=QVBoxLayout(timeline_page); timeline_layout.setContentsMargins(0,0,0,0); timeline_layout.addWidget(self.timeline_empty); timeline_layout.addWidget(self.timeline)
        offices_page=QWidget(); offices_layout=QVBoxLayout(offices_page); offices_layout.setContentsMargins(0,0,0,0); offices_layout.addWidget(self.offices_empty); offices_layout.addWidget(self.offices)
        self.log_tabs.addTab(timeline_page,'Таймлайн'); self.log_tabs.addTab(offices_page,'Доступные офисы')
        self.log_tabs.currentChanged.connect(self.update_copy_label)
        journals_layout.addWidget(self.log_tabs); outer.addWidget(journals,1); journals.setMinimumHeight(200)
        from ..version import VERSION
        self.footer = label('Данные хранятся локально · '+VERSION, muted=True); outer.addWidget(self.footer)

    def empty_journal(self,text,icon):
        container=QWidget()
        layout=QVBoxLayout(container);layout.setContentsMargins(24,24,24,24);layout.setSpacing(12)
        layout.addStretch()
        image=QLabel();image.setFixedSize(28,28)
        self.empty_icon_labels.append((image,icon))
        layout.addWidget(image,alignment=Qt.AlignmentFlag.AlignHCenter)
        message=label(text,muted=True);message.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(message)
        layout.addStretch()
        return container

    def table(self, headers):
        table = JournalTable(0,len(headers));table.setObjectName("journalTable"); table.setHorizontalHeaderLabels(headers)
        table.setWordWrap(False); table.setShowGrid(False)
        table.setEditTriggers(QAbstractItemView.EditTrigger.NoEditTriggers)
        table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        table.setSelectionMode(QAbstractItemView.SelectionMode.ExtendedSelection)
        table.itemSelectionChanged.connect(self.update_copy_label)
        copy_action=QAction(table)
        copy_action.setShortcut(QKeySequence.StandardKey.Copy)
        copy_action.setShortcutContext(Qt.ShortcutContext.WidgetWithChildrenShortcut)
        copy_action.triggered.connect(self.copy_log)
        table.addAction(copy_action)
        table.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        table.customContextMenuRequested.connect(lambda point: self.log_copy_menu(table,point))
        table.setTextElideMode(Qt.TextElideMode.ElideRight)
        table.verticalHeader().hide(); table.verticalHeader().setDefaultSectionSize(40)
        table.horizontalHeader().setStretchLastSection(False)
        table.horizontalHeader().setDefaultAlignment(Qt.AlignmentFlag.AlignLeft|Qt.AlignmentFlag.AlignVCenter)
        table.setHorizontalScrollMode(QAbstractItemView.ScrollMode.ScrollPerPixel)
        table.setColumnWidth(0,131)
        if len(headers)==4:
            table.setColumnWidth(1,196); table.setColumnWidth(2,116)
            table.detail_minimum=317
        else:table.detail_minimum=629
        table.setColumnWidth(len(headers)-1,table.detail_minimum)
        return table

    def _statistics(self):
        from .statistics_panel import StatisticsPanel
        self.statistics_panel=StatisticsPanel(self)
        scroll=QScrollArea();scroll.setWidgetResizable(True);scroll.setFrameShape(QScrollArea.Shape.NoFrame)
        scroll.setWidget(self.statistics_panel);self.tabs.insertTab(1,scroll,'Статистика')

    def _settings(self):
        scroll = QScrollArea(); scroll.setWidgetResizable(True); scroll.setFrameShape(QScrollArea.Shape.NoFrame)
        page=QWidget(); layout=QVBoxLayout(page); layout.setContentsMargins(28,25,28,25); layout.setSpacing(16)
        scroll.setWidget(page); self.tabs.addTab(scroll,'Настройки')
        layout.addWidget(label('Настройки',name='pageHeading'))
        layout.addWidget(label('Профили',name='sectionHeading'))
        toolbar=QHBoxLayout()
        self.settings_profile_combo=QComboBox();self.settings_profile_combo.setObjectName('settingsProfile')
        self.settings_profile_combo.setMinimumWidth(200);self.settings_profile_combo.setMaximumWidth(280)
        self.settings_profile_combo.currentIndexChanged.connect(self.update_profile_summary)
        toolbar.addWidget(self.settings_profile_combo)
        edit=QPushButton('Редактировать');edit.setObjectName('editProfile');edit.clicked.connect(self.edit_profile);toolbar.addWidget(edit)
        create=QPushButton('Создать профиль');create.setObjectName('createProfile');create.clicked.connect(self.create_profile);toolbar.addWidget(create)
        toolbar.addStretch();layout.addLayout(toolbar)
        self.profile_summary=label('',muted=True);layout.addWidget(self.profile_summary)
        self.profiles_empty=label('Профилей пока нет. Создать профиль с данными заявителя и способом авторизации.',muted=True)
        layout.addWidget(self.profiles_empty)
        self.profile_editor=QDialog(self); self.profile_editor.setModal(True); self.profile_editor.resize(650,580)
        form=QVBoxLayout(self.profile_editor); form.setContentsMargins(24,20,24,20); form.setSpacing(12)
        self.editor_title=label('Редактирование профиля',name='sectionHeading'); form.addWidget(self.editor_title)
        grid=QGridLayout(); grid.setHorizontalSpacing(12); grid.setVerticalSpacing(8); grid.setAlignment(Qt.AlignmentFlag.AlignTop)
        self.fields={};self.profile_grid=grid
        self._document_drafts={};self._document_kind='NIE';self._regreso_document_type='NIE'
        self._editor_procedure=TIE_PROCEDURE;self._identity_layout_service=None
        fields=[('label','Название профиля'),('province','Провинция'),('procedure','Услуга'),('access_mode','Способ авторизации'),
                ('fill_mode','NIE и имя'),('document_type','Документ'),('document_number','NIE'),('full_name','Имя и фамилия'),
                ('nationality','Гражданство'),('phone','Телефон'),('email','Почта'),
                ('delay_min','Задержка действий: минимум, с'),
                ('delay_max','Задержка действий: максимум, с')]
        countries=sorted(_load_catalog(self.store.catalog_path)['nationalities'])
        positions={'label':(0,0,1),'province':(0,1,1),'procedure':(1,0,1),'access_mode':(1,1,1),
                   'fill_mode':(2,0,2),'document_type':(3,0,1),'document_number':(3,1,1),'full_name':(4,0,2),
                   'nationality':(6,0,2),'phone':(5,0,1),'email':(5,1,1),
                   'delay_min':(7,0,1),'delay_max':(7,1,1)}
        grid.setColumnStretch(0,1);grid.setColumnStretch(1,1)
        self.profile_field_boxes={}
        self.identity_buttons={}
        identity_group=QButtonGroup(self.profile_editor);identity_group.setExclusive(True)
        for index,(key,title) in enumerate(fields):
            if key in {'access_mode','fill_mode','nationality','province','procedure','document_type'}:
                widget=ProfileComboBox()
                if key=='access_mode':
                    widget.addItem('Электронный сертификат','certificate'); widget.addItem('Cl@ve Móvil','clave_movil'); widget.addItem('Cl@ve Permanente','clave_permanente')
                elif key=='fill_mode':
                    widget.addItem('Ввести данные профиля','manual'); widget.addItem('Скопировать с сайта','copy')
                elif key=='nationality':
                    widget.addItems(countries); widget.setEditable(True)
                elif key=='province': widget.addItems(sorted(_load_catalog(self.store.catalog_path)['provinces']))
                elif key=='document_type':
                    for text,value in [('NIE','NIE'),('DNI','DNI'),('Паспорт','PASSPORT')]:widget.addItem(text,value)
                else:
                    for value,text in PROCEDURE_NAMES.items():widget.addItem(text,value)
            elif key in {'delay_min', 'delay_max'}:
                widget=QDoubleSpinBox()
                widget.setRange(0,10); widget.setDecimals(2); widget.setSingleStep(.1)
            else:
                widget=QLineEdit()
                if key=='document_number': widget.setMaxLength(20)
            widget.setAccessibleName(title); self.fields[key]=widget
            if key=='fill_mode':
                box=QWidget(self.profile_editor);self.identity_box=box;column=QVBoxLayout(box);column.setContentsMargins(0,0,0,0);column.setSpacing(6)
                mode=QHBoxLayout();mode.setSpacing(8);self.identity_caption=label('NIE и имя',muted=True);mode.addWidget(self.identity_caption);self.identity_caption.hide()
                for value,text in [('copy','Из авторизации'),('manual','Ввести вручную')]:
                    button=QPushButton(text);button.setCheckable(True);button.setFocusPolicy(Qt.FocusPolicy.StrongFocus);button.setProperty('profileMode',True)
                    button.clicked.connect(lambda checked,v=value:self.fields['fill_mode'].setCurrentIndex(self.fields['fill_mode'].findData(v)))
                    identity_group.addButton(button);self.identity_buttons[value]=button;mode.addWidget(button)
                mode.addStretch();column.addLayout(mode);widget.setParent(box);widget.hide()
                self.identity_note=label('NIE и имя будут взяты после входа на сайт.',muted=True);column.addWidget(self.identity_note)
            elif key in {'delay_min','delay_max'}:
                box=QWidget();column=QVBoxLayout(box);column.setContentsMargins(0,0,0,0);column.addWidget(widget)
            else:
                box=ProfileField(title,widget);self.profile_field_boxes[key]=box
            row,col,span=positions[key]
            grid.addWidget(box,row,col,1,span)
        self.document_fields_row=QWidget();self.document_fields_layout=QHBoxLayout(self.document_fields_row)
        self.document_fields_layout.setContentsMargins(0,0,0,0);self.document_fields_layout.setSpacing(12)
        self.profile_field_boxes['document_type'].setFixedWidth(160)
        self.profile_focus_fields=[self.fields[key] for key in ('label','province','procedure','access_mode')]+[self.identity_buttons['copy'],self.identity_buttons['manual']]+[self.fields[key] for key in ('document_type','document_number','full_name','nationality','phone','email')]
        self.fields['procedure'].currentIndexChanged.connect(self.update_profile_document)
        self.fields['document_type'].currentIndexChanged.connect(self.update_profile_document)
        self.fields['access_mode'].currentIndexChanged.connect(self.update_identity_fields)
        self.fields['fill_mode'].currentIndexChanged.connect(self.update_identity_fields)
        profile_tabs=QTabWidget(); self.profile_tabs=profile_tabs
        details=QWidget(); details.setLayout(grid)
        self.update_identity_fields()
        details_scroll=QScrollArea(); details_scroll.setWidgetResizable(True); details_scroll.setFrameShape(QScrollArea.Shape.NoFrame); details_scroll.setWidget(details)
        profile_tabs.addTab(details_scroll,'Данные')
        timings_page=QWidget(); timing_layout=QGridLayout(timings_page)
        timing_layout.setSpacing(10); timing_layout.setSizeConstraint(QLayout.SizeConstraint.SetMinimumSize)
        self.timing_widgets={}
        from ..timings import AssistTimings, TIMING_FIELDS, SEQUENCE_KEYS
        self.timing_field_boxes={}
        self.timing_focus_fields=[]
        short_titles={
            'action_delay_seconds':'Между действиями','typing_ms':'Ввод NIE и имени','contact_typing_ms':'Ввод контактов',
            'reload_seconds':'Интервал проверки','navigation_seconds':'Таймаут страницы','bootstrap_seconds':'Начальная загрузка / F5',
            'no_slots_restart_seconds':'Новый проход · Aceptar','clave_error_seconds':'Ошибка Cl@ve','rate_limit_seconds':'HTTP 429',
            'recovery_jitter_seconds':'Добавочная пауза','expired_seconds':'Истёкшая сессия',
            'server_button_seconds':'Кнопка ошибки','server_retry_seconds':'Ошибка без кнопки','blocked_seconds':'Блокировка',
            'timeout_retry_seconds':'Клик не отправлен','post_timeout_seconds':'Ожидание после клика',
            'restart_seconds':'Перезапуск после таймаута','alert_seconds':'Повтор звонка'}
        timing_row=0;timing_column=0
        defaults=AssistTimings()
        timing_groups=[('Ввод данных', {'action_delay_seconds','typing_ms','contact_typing_ms'}),
            ('Проверка сит', {'reload_seconds','navigation_seconds','bootstrap_seconds'}),
            ('Восстановление', {key for key, *_ in TIMING_FIELDS} - {'action_delay_seconds','typing_ms','contact_typing_ms','reload_seconds','navigation_seconds','bootstrap_seconds','alert_seconds'}),
            ('Уведомления', {'alert_seconds'})]
        ordered=[]
        for group, keys in timing_groups:
            ordered.append((None,group,0,0))
            if group=='Проверка сит':ordered.append(('no_slots_mode','Повторная проверка',0,0))
            ordered.extend(field for field in TIMING_FIELDS if field[0] in keys)
        for key,title,lower,upper in ordered:
            if key is None:
                if timing_column:timing_row+=1;timing_column=0
                heading=label(title,name='sectionHeading');heading.setContentsMargins(0,8,0,0)
                timing_layout.addWidget(heading,timing_row,0,1,2);timing_row+=1;continue
            if key=='no_slots_mode':
                self.build_search_behavior(timing_layout,timing_row)
                timing_row+=4
                continue
            if key=='action_delay_seconds':
                widgets=[self.fields['delay_min'],self.fields['delay_max']]
                # Move action inputs from the details grid into this tab.
                for widget in widgets:
                    widget.parentWidget().hide(); widget.setParent(timings_page)
            elif key in SEQUENCE_KEYS:
                widgets=[QLineEdit()]; widgets[0].setPlaceholderText(', '.join(str(v) for v in getattr(defaults,key)))
            else:
                widgets=[]
                for _ in range(2 if isinstance(getattr(defaults,key),tuple) else 1):
                    widget=QDoubleSpinBox(); widget.setRange(lower,upper); widget.setDecimals(2); widget.setSingleStep(.1 if upper<=10 else 1); widgets.append(widget)
            for index,widget in enumerate(widgets):
                widget.setAccessibleName(title+(' · min' if index==0 and len(widgets)==2 else ' · max' if index==1 else ''))
                if isinstance(widget,QDoubleSpinBox):widget.setAlignment(Qt.AlignmentFlag.AlignCenter if len(widgets)==2 else Qt.AlignmentFlag.AlignLeft)
                else:widget.setAlignment(Qt.AlignmentFlag.AlignLeft)
            row_box=TimingField(short_titles[key],widgets,'ms' if key.endswith('_ms') else 's')
            row_box.help_text=title
            row_box.help.setToolTip(self.t(title))
            self.timing_field_boxes[key]=row_box
            self.timing_widgets[key]=widgets;self.timing_focus_fields.extend(widgets);timing_layout.addWidget(row_box,timing_row,timing_column)
            timing_column=1-timing_column
            if timing_column==0:timing_row+=1
        for previous,current in zip(self.timing_focus_fields,self.timing_focus_fields[1:]):QWidget.setTabOrder(previous,current)
        timing_layout.setColumnStretch(0,1);timing_layout.setColumnStretch(1,1)
        timing_layout.setRowStretch(timing_row+1,1)
        timing_scroll=QScrollArea(); timing_scroll.setWidgetResizable(True); timing_scroll.setFrameShape(QScrollArea.Shape.NoFrame); timing_scroll.setWidget(timings_page)
        profile_tabs.addTab(timing_scroll,'Тайминги и повторы')
        from .notification_widgets import NotificationPanel
        self.notifications_panel=NotificationPanel(self)
        profile_tabs.addTab(self.notifications_panel.scroll,'Уведомления'); form.addWidget(profile_tabs,1)
        for previous,current in zip(self.profile_focus_fields,self.profile_focus_fields[1:]):
            QWidget.setTabOrder(previous,current)
        profile_tabs.currentChanged.connect(self.profile_tab_changed)
        form.addWidget(label('Пароли и подтверждения — только в Camoufox.',muted=True))
        self.form_error=label(name='formError'); form.addWidget(self.form_error)
        actions=QHBoxLayout(); actions.addStretch(); save=QPushButton('Сохранить профиль');save.setObjectName('saveProfile'); save.setProperty('primary',True)
        save.setDefault(True); save.clicked.connect(self.save_profile); actions.addWidget(save)
        cancel=QPushButton('Отмена');cancel.setObjectName('cancelProfile');cancel.clicked.connect(self.profile_editor.reject); actions.addWidget(cancel)
        form.addLayout(actions); self.profile_editor.hide()
        layout.addWidget(label('Диагностика и звук',name='sectionHeading'))
        self.debug_toggle=self.toggle_row(layout,'Запись диагностики','Сохранять страницы, снимки и ошибки. Таймлайн записывается всегда.',key='diagnostics')
        self.sound_toggle=self.toggle_row(layout,'Звуковые уведомления','Сигнал повторяется, пока требуется участие: выбор офиса, капча или подтверждение брони. Интервал — в «Таймингах» профиля.',key='sound')
        folder=QPushButton('Открыть папку диагностики');folder.setObjectName('openDiagnostics'); folder.clicked.connect(self.open_debug_folder)
        layout.addWidget(folder,alignment=Qt.AlignmentFlag.AlignLeft)
        layout.addWidget(label('Обновления',name='sectionHeading'))
        self.updates_toggle=self.toggle_row(layout,'Автоматически проверять обновления','Проверять GitHub Releases при запуске и раз в сутки. Скачивание и установка — по кнопке.',key='updates')
        layout.addWidget(label('Интерфейс',name='sectionHeading'))
        self.settings_language=QComboBox()
        for code,title in LANGUAGES.items(): self.settings_language.addItem(code.upper()+' · '+title,code)
        self.settings_language.currentIndexChanged.connect(lambda:self.set_language(self.settings_language.currentData()))
        layout.addWidget(self.settings_language)
        self.settings_theme=self.toggle_row(layout,'Тёмная тема','Изменить оформление приложения',key='theme')
        self.settings_theme.toggled.connect(lambda checked:self.dark_toggle.setChecked(checked))
        layout.addStretch()

    def toggle_row(self, layout, title, hint, *, key):
        container=QWidget(); container.setObjectName('settingRow')
        row=QHBoxLayout(container); row.setContentsMargins(16,14,16,14); row.setSpacing(24)
        text=QVBoxLayout(); text.setSpacing(5)
        title_label=label(title,name='settingTitle_'+key); title_label.setWordWrap(False)
        text.addWidget(title_label); text.addWidget(label(hint,muted=True))
        row.addLayout(text,1); toggle=Toggle(title); row.addWidget(toggle)
        layout.addWidget(container)
        return toggle

    def refresh_profiles(self, selected=None):
        previous=self.profile_combo.currentData() if self.controller.active else (selected or self.profile_combo.currentData())
        try:
            previous=previous or self.store.last_profile()
            self.profiles=self.store.load()
        except ProfileValidationError as exc:
            self.profiles=[]; self.show_error(str(exc),'profiles')
        else: self.clear_error('profiles')
        self.profiles_empty.setVisible(not self.profiles)
        self.settings_profile_combo.blockSignals(True); self.profile_combo.blockSignals(True); self.profile_combo.clear(); self.settings_profile_combo.clear()
        for task in self.profiles:
            self.profile_combo.addItem(task.label,task.id)
            self.settings_profile_combo.addItem(task.label + (self.t(' — проверить данные') if task.errors else ''),task.id)
        if previous:
            self.profile_combo.setCurrentIndex(max(0,self.profile_combo.findData(previous)))
        self.profile_combo.blockSignals(False)
        self.profile_changed()
        self.statistics_panel.set_profiles(self.profiles,self.profile_combo.currentData())
        self.settings_profile_combo.setCurrentIndex(max(0,self.settings_profile_combo.findData(previous)))
        self.settings_profile_combo.blockSignals(False)
        self.update_profile_summary()
        self.update_start()

    def main_tab_changed(self,index):
        if index==1:
            panel=self.statistics_panel
            selected=panel.profile_combo.findData(self.profile_combo.currentData())
            if selected>=0:panel.profile_combo.setCurrentIndex(selected)

    def mark_success_from_work(self):
        panel=self.statistics_panel
        index=panel.profile_combo.findData(self.profile_combo.currentData())
        if index>=0:panel.profile_combo.setCurrentIndex(index)
        self.tabs.setCurrentIndex(1);panel.confirm_success()

    def profile_check_total(self):
        from .statistics import ProfileStatistics
        try:
            return ProfileStatistics(self.store.path.parent / 'data' / 'profile-statistics.json').total(self.profile_combo.currentData())
        except (OSError, ValueError):
            return None

    def profile_changed(self):
        profile_id=self.profile_combo.currentData()
        if profile_id and any(task.id==profile_id for task in self.profiles):
            try:self.store.save_last_profile(profile_id)
            except (ProfileValidationError,ProfileConflictError,OSError):
                self.show_error('Не удалось сохранить настройки. Изменения не применены.','last_profile')
            else:self.clear_error('last_profile')
        if not self.controller.active and hasattr(self, 'metrics'):
            total = self.profile_check_total()
            self.metrics.setText(f"Проверки: 0 за запуск · Всего: {total if total is not None else '—'}")
            self.retranslate()
        self.update_start()
        task=next((t for t in self.profiles if t.id==self.profile_combo.currentData()),None)
        self.service_label.setText(self.t('Услуга')+': '+PROCEDURE_NAMES.get(task.procedure,task.procedure) if task else '')
        self.service_label.setVisible(task is not None);self.service_icon.setVisible(task is not None)
        self.restart_action.setEnabled(bool(self.profile_combo.currentData()) and not self.state.get('browser_retained',False))

    def update_start(self):
        if not hasattr(self,'profiles'): return
        task=next((t for t in self.profiles if t.id==self.profile_combo.currentData()),None)
        self.start_button.setEnabled(self.controller.active or bool(task and not task.errors and task.access_mode in {'clave','certificate'}))

    def update_profile_document(self, *_):
        procedure=self.fields['procedure'].currentData()
        regreso=procedure==REGRESO_PROCEDURE
        if self._editor_procedure==REGRESO_PROCEDURE:
            self._regreso_document_type=self.fields['document_type'].currentData()
        kind=self._regreso_document_type if regreso else 'NIE'
        self._document_drafts[self._document_kind]=self.fields['document_number'].text()
        self._document_kind=kind;self._editor_procedure=procedure
        self.fields['document_number'].setText(self._document_drafts.get(kind,''))
        combo=self.fields['document_type'];combo.blockSignals(True)
        combo.setCurrentIndex(max(0,combo.findData(kind)));combo.blockSignals(False)
        self.update_identity_fields()

    def update_identity_fields(self, *_):
        manual=self.fields['fill_mode'].currentData() != 'copy'
        regreso=self.fields['procedure'].currentData()==REGRESO_PROCEDURE
        if self._identity_layout_service!=regreso:
            for key in ('document_type','document_number','full_name','nationality'):
                box=self.profile_field_boxes[key];self.profile_grid.removeWidget(box);self.document_fields_layout.removeWidget(box)
            self.profile_grid.removeWidget(self.document_fields_row)
            if regreso:
                self.document_fields_layout.addWidget(self.profile_field_boxes['document_type'])
                self.document_fields_layout.addWidget(self.profile_field_boxes['document_number'],1)
                self.profile_grid.addWidget(self.document_fields_row,3,0,1,2)
                self.profile_grid.addWidget(self.profile_field_boxes['full_name'],4,0,1,2)
            else:
                self.profile_grid.addWidget(self.profile_field_boxes['document_number'],3,0)
                self.profile_grid.addWidget(self.profile_field_boxes['full_name'],3,1)
                self.profile_grid.addWidget(self.profile_field_boxes['nationality'],4,0,1,2)
            self._identity_layout_service=regreso
        self.document_fields_row.setVisible(regreso and manual)
        self.profile_field_boxes['document_type'].setVisible(regreso and manual)
        self.profile_field_boxes['nationality'].setVisible(not regreso)
        self.identity_caption.setText(self.t('Документ и имя' if regreso else 'NIE и имя'))
        self.identity_note.setText(self.t('Документ и имя будут взяты после входа на сайт.' if regreso else 'NIE и имя будут взяты после входа на сайт.'))
        number_title=({'NIE':'Номер NIE','DNI':'Номер DNI','PASSPORT':'Номер паспорта'}.get(self.fields['document_type'].currentData(),'Номер документа') if regreso else 'NIE')
        self.profile_field_boxes['document_number'].caption.setText(self.t(number_title))
        self.fields['document_number'].setPlaceholderText(self.t(number_title))
        self.fields['document_number'].setAccessibleName(self.t(number_title))
        for key in ('document_number','full_name'):
            self.fields[key].setEnabled(manual)
            self.profile_field_boxes[key].setVisible(manual)
            self.fields[key].setToolTip(self.t('Берётся из текущей авторизации на сайте') if not manual else '')

        self.identity_note.setVisible(not manual)
        for value,button in self.identity_buttons.items():
            selected=value==('manual' if manual else 'copy')
            button.setChecked(selected)
            button.setFocusPolicy(Qt.FocusPolicy.StrongFocus if selected else Qt.FocusPolicy.ClickFocus)
        for previous,current in zip(self.profile_focus_fields,self.profile_focus_fields[1:]):
            QWidget.setTabOrder(previous,current)

    def create_profile(self):
        self.open_editor(None)

    def edit_profile(self, *_):
        profile_id=self.settings_profile_combo.currentData()
        if profile_id: self.open_editor(profile_id)

    def update_profile_summary(self, *_):
        profile_id=self.settings_profile_combo.currentData()
        self.findChild(QPushButton,'editProfile').setEnabled(bool(profile_id))
        if not profile_id:
            self.profile_summary.clear()
            return
        request=self.store.raw_profile(profile_id).get('request',{})
        auth=('Электронный сертификат' if request.get('access_mode','certificate')=='certificate'
              else 'Cl@ve Permanente' if request.get('clave_method','movil')=='permanente' else 'Cl@ve Móvil')
        self.profile_summary.setText(str(request.get('province',''))+' · '+PROCEDURE_NAMES.get(request.get('procedure',TIE_PROCEDURE),'')+' · '+self.t(auth))

    def build_search_behavior(self, layout, row):
        self.applicant_no_slots_combo=QComboBox()
        self.applicant_no_slots_combo.setObjectName('profileApplicantNoSlotsMode')
        self.applicant_no_slots_combo.addItem('Ждать при «нет сит»','wait')
        self.applicant_no_slots_combo.addItem('Вводить и продолжать','continue')
        self.no_slots_mode_combo=QComboBox()
        self.no_slots_mode_combo.setObjectName('profileNoSlotsMode')
        self.no_slots_mode_combo.addItem('Обновлять результат','reload')
        self.no_slots_mode_combo.addItem('Через Salir','salir')
        self.applicant_no_slots_box=ProfileField('На форме заявителя',self.applicant_no_slots_combo)
        self.no_slots_mode_box=ProfileField('После ответа «нет сит»',self.no_slots_mode_combo)
        self.search_mode_descriptions=[]
        for col,(combo,box) in enumerate(((self.applicant_no_slots_combo,self.applicant_no_slots_box),(self.no_slots_mode_combo,self.no_slots_mode_box))):
            combo.setMinimumWidth(0)
            self.timing_focus_fields.append(combo)
            layout.addWidget(box,row,col)
            description=label('',muted=True);description.setWordWrap(True)
            description.setMinimumWidth(0);description.setAlignment(Qt.AlignmentFlag.AlignLeft|Qt.AlignmentFlag.AlignTop)
            description.setSizePolicy(QSizePolicy.Policy.Ignored,QSizePolicy.Policy.Minimum)
            self.search_mode_descriptions.append(description);layout.addWidget(description,row+1,col)
            combo.currentIndexChanged.connect(self.update_search_behavior)
        self.search_behavior_note=QFrame();self.search_behavior_note.setObjectName('searchBehaviorNote')
        note=QHBoxLayout(self.search_behavior_note);note.setContentsMargins(12,11,12,11);note.setSpacing(10)
        self.search_behavior_icon=QLabel();self.search_behavior_icon.setAlignment(Qt.AlignmentFlag.AlignTop);note.addWidget(self.search_behavior_icon)
        content=QVBoxLayout();content.setSpacing(4)
        self.search_behavior_title=label('');self.search_behavior_title.setObjectName('searchBehaviorTitle')
        self.search_behavior_text=label('',muted=True);self.search_behavior_text.setWordWrap(True)
        content.addWidget(self.search_behavior_title);content.addWidget(self.search_behavior_text);note.addLayout(content,1)
        layout.addWidget(self.search_behavior_note,row+2,0,1,2)
        self.search_behavior_hint=label('Выбор поведения не меняет интервалы и паузы восстановления.',muted=True)
        self.search_behavior_hint.setWordWrap(True);layout.addWidget(self.search_behavior_hint,row+3,0,1,2)

    def update_search_behavior(self,*_):
        enter=self.applicant_no_slots_combo.currentData()=='continue'
        reload=self.no_slots_mode_combo.currentData()=='reload'
        descriptions=(
            'Вводить или копировать данные даже при предупреждении «нет сит».' if enter else 'Не вводить данные, пока форма сообщает об отсутствии сит.',
            'После паузы повторять запрос доступности в текущей сессии.' if reload else 'После паузы нажать Salir и продолжить по странице сайта.')
        for widget,text in zip(self.search_mode_descriptions,descriptions):widget.setText(self.t(text))
        risk=enter or reload
        for widget in (self.search_behavior_note,self.search_behavior_title):
            widget.setProperty('risk',risk)
            widget.style().unpolish(widget);widget.style().polish(widget)
        self.search_behavior_title.setText(self.t('Возможны блокировки' if risk else 'Рекомендуется для обычного поиска'))
        self.search_behavior_text.setText(self.t('Повторная отправка данных может повысить риск блокировки' if risk else 'Пока сайт сообщает, что сит нет, данные не отправляются повторно. Для следующей проверки используется штатная кнопка Salir.'))
        self.search_behavior_icon.setPixmap(self.control_icon('shield' if risk else 'check').pixmap(QSize(18,18)))

    def profile_tab_changed(self,index):
        if index==self.profile_tabs.indexOf(self.notifications_panel.scroll):
            self.notifications_panel.ensure_secrets_loaded()
        self.update_editor_height()

    def open_editor(self, profile_id):
        self.editing_id=profile_id; self.editor_revision=self.store.revision()
        raw=self.store.raw_profile(profile_id) if profile_id else {}
        self.notifications_panel.load(raw.get('notifications',{}),defer_secrets=True)
        self.no_slots_mode_combo.setCurrentIndex(max(0,self.no_slots_mode_combo.findData(raw.get('request',{}).get('no_slots_mode','salir'))))
        self.applicant_no_slots_combo.setCurrentIndex(max(0,self.applicant_no_slots_combo.findData(raw.get('request',{}).get('applicant_no_slots_mode','wait'))))
        self.update_search_behavior()
        applicant=raw.get('applicant',{}); contact=applicant.get('contact',{})
        values=dict(label=raw.get('label',''),full_name=applicant.get('full_name',''),
            document_number=applicant.get('document_number',''),phone=contact.get('phone',''),email=contact.get('email',''))
        for key,value in values.items(): self.fields[key].setText(str(value))
        procedure=raw.get('request',{}).get('procedure',TIE_PROCEDURE)
        document_type=applicant.get('document_type','NIE')
        if document_type not in {'NIE','DNI','PASSPORT'}:document_type='NIE'
        for key,value in [('procedure',procedure),('document_type',document_type if procedure==REGRESO_PROCEDURE else 'NIE')]:
            combo=self.fields[key];combo.blockSignals(True)
            combo.setCurrentIndex(max(0,combo.findData(value)));combo.blockSignals(False)
        self._editor_procedure=procedure;self._document_kind=document_type if procedure==REGRESO_PROCEDURE else 'NIE'
        self._regreso_document_type=document_type;self._document_drafts={self._document_kind:str(values['document_number'])}
        self.fields['nationality'].setCurrentText(applicant.get('nationality',''))
        self.fields['province'].setCurrentText(raw.get('request',{}).get('province','Barcelona'))
        access=raw.get('request',{}).get('access_mode','certificate')
        if access=='clave':
            access='clave_'+raw.get('request',{}).get('clave_method','movil')
        self.fields['access_mode'].setCurrentIndex(max(0,self.fields['access_mode'].findData(access)))
        self.fields['fill_mode'].setCurrentIndex(max(0,self.fields['fill_mode'].findData(applicant.get('fill_mode','manual'))))
        try:
            low, high = self.store.action_timings(raw)
        except ProfileValidationError:
            low, high = .3, .8
        self.fields['delay_min'].setValue(low); self.fields['delay_max'].setValue(high)
        from ..timings import AssistTimings, SEQUENCE_KEYS
        try: timings=self.store.timings(raw)
        except ProfileValidationError: timings=AssistTimings()
        for key,widgets in self.timing_widgets.items():
            value=getattr(timings,key)
            if key in SEQUENCE_KEYS: widgets[0].setText(', '.join(f'{v:g}' for v in value))
            elif isinstance(value,tuple):
                for widget,number in zip(widgets,value): widget.setValue(number)
            else: widgets[0].setValue(value)
        self.update_identity_fields()
        self.form_error.clear(); self.editor_title.setText('Редактирование профиля' if profile_id else 'Создание профиля')
        self.tabs.setCurrentIndex(1); self.profile_editor.setWindowTitle(self.t('Редактирование профиля' if profile_id else 'Создание профиля')); self.profile_editor.show()
        self.retranslate()
        self.profile_tab_changed(self.profile_tabs.currentIndex())

    def save_profile(self):
        f=self.fields
        from ..timings import SEQUENCE_KEYS
        timings={}
        try:
            for key,widgets in self.timing_widgets.items():
                if key in SEQUENCE_KEYS:
                    value=widgets[0].text().strip()
                    timings[key]=[float(v.strip()) for v in value.split(',')] if value else None
                else:
                    values=[w.value() for w in widgets]; timings[key]=values if len(values)==2 else values[0]
        except ValueError:
            self.form_error.setText(self.t('Паузы попыток: ввести числа через запятую.')); return
        selected_auth=f['access_mode'].currentData()
        access_mode='clave' if selected_auth.startswith('clave_') else 'certificate'
        clave_method=selected_auth.removeprefix('clave_') if access_mode=='clave' else 'movil'
        data=dict(label=f['label'].text().strip(),timings=timings,request=dict(access_mode=access_mode,clave_method=clave_method,no_slots_mode=self.no_slots_mode_combo.currentData(),applicant_no_slots_mode=self.applicant_no_slots_combo.currentData(),
            province=f['province'].currentText(),office='',procedure=f['procedure'].currentData(),subprocedure=None),
            applicant=dict(fill_mode=f['fill_mode'].currentData(),document_type=f['document_type'].currentData() if f['procedure'].currentData()==REGRESO_PROCEDURE else 'NIE',document_number=f['document_number'].text(),
                full_name=f['full_name'].text(),nationality=f['nationality'].currentText(),
                contact=dict(phone=f['phone'].text(),email=f['email'].text())))
        from ..notifications import NotificationError
        created=[];old=[]
        try:
            notifications,created,old=self.notifications_panel.prepare()
            data['notifications']=notifications
            profile_id=self.store.save_profile(self.editing_id,data,revision=self.editor_revision)
        except (ProfileValidationError,ProfileConflictError,OSError,NotificationError) as exc:
            for ref in created:self.notifications_panel.vault.delete(ref)
            self.form_error.setText(str(exc) if not isinstance(exc,OSError) else 'Не удалось сохранить файл профилей.'); self.retranslate(); return
        for ref in old:self.notifications_panel.vault.delete(ref)
        if self.notification_delivery.profile_id==profile_id:self.notification_delivery.generation+=1
        self.profile_editor.accept(); self.refresh_profiles(profile_id)
        self.statusBar().showMessage(self.t('Профиль сохранён'),5000)

    def save_preferences(self):
        previous_revision=self.store.revision()
        preferences=dict(dark=self.dark_toggle.isChecked(),diagnostics=self.debug_toggle.isChecked(),sound=self.sound_toggle.isChecked(),language=getattr(self,'pending_language',self.preferences['language']),automatic_updates=self.updates_toggle.isChecked())
        try:
            self.store.save_preferences(**preferences)
        except (ProfileValidationError,ProfileConflictError,OSError):
            self.show_error('Не удалось сохранить настройки. Изменения не применены.','preferences')
            for key,toggle in [('dark',self.dark_toggle),('diagnostics',self.debug_toggle),('sound',self.sound_toggle),('automatic_updates',self.updates_toggle)]:
                toggle.blockSignals(True); toggle.setChecked(self.preferences[key]); toggle.blockSignals(False)
            self.pending_language=self.preferences['language']
            return
        if self.editor_revision==previous_revision: self.editor_revision=self.store.revision()
        self.clear_error('preferences')
        self.preferences=preferences
        apply_theme(QApplication.instance(),dark=preferences['dark'])
        self.controller.send(dict(command='preferences',diagnostics=preferences['diagnostics'],sound=preferences['sound']))
        from ..version import VERSION
        self.footer.setText('Данные хранятся локально · '+VERSION)
        self.retranslate()

    def reset_session(self):
        if self.controller.active:
            self.controller.send({'command':'restart'}); return
        profile_id=self.profile_combo.currentData()
        if not profile_id:return
        from ..site import reset_browser_profile, SiteCheckError
        try:
            reset_browser_profile(self.store.settings(profile_id),profile_id)
        except (SiteCheckError,OSError,ProfileValidationError) as exc:
            self.show_error(str(exc),'reset');return
        self.clear_error('reset')
        self.statusBar().showMessage(self.t('Сессия сброшена. Можно запустить поиск.'),7000)

    def start_or_pause(self):
        if self.state.get('booking_handoff'):return
        if self.controller.active:
            self.controller.send(dict(command='pause',paused=not self.state.get('paused',False)))
        else:
            self.clear_error('worker')
            self.notification_delivery.reset()
            self.controller.start(self.profile_combo.currentData())
            self.state={}
            self.update_application_icon()

    def update_application_icon(self):
        from .status_icon import application_icon, icon_status
        status=icon_status(self.state,self.controller.active,'worker' in self.errors)
        if status not in self._status_icons:
            self._status_icons[status]=application_icon(status)
        if self.icon_status != status or self.windowIcon().isNull():
            self.icon_status=status
            self.setWindowIcon(self._status_icons[status])
            QApplication.instance().setWindowIcon(self._status_icons[status])

    def notification_result(self,channel,message):
        self.statusBar().showMessage(channel.title()+': '+self.t(message),7000)

    def on_state(self, state):
        profile_id=getattr(self.controller,'profile_id',None) or self.profile_combo.currentData()
        self.notification_delivery.observe(profile_id,state,self.preferences['language'])
        previous_mode=self.state.get('mode')
        self.state=state
        self.heading.setText('Сит нет' if state.get('screen')=='Сит нет — требуется новый проход' else state.get('screen','Ищем ситу'))
        self.evidence.setText(state.get('evidence',''))
        self.next_label.setText(state.get('next_action',''))
        mode={'working':'Бот работает','waiting':'Ожидание','paused':'Приостановлен','manual':'Требуется участие','recovery':'Восстановление','finished':'Остановлен'}.get(state.get('mode'),'Запуск')
        if state.get('booking_handoff'):mode='Управление передано'
        self.status_label.setText('Связь с ботом потеряна' if state.get('connection_lost') else mode)
        remaining=state.get('remaining_seconds')
        self.timer_label.setText('—' if remaining is None or state.get('connection_lost') else f'{int(remaining)//60:02d}:{int(remaining)%60:02d}')
        self.metrics.setText(f"Проверки: {state.get('checks',0)} за запуск · Всего: {state.get('profile_checks',0)}")
        self.notice.setText('Показано последнее состояние. Проверить процесс бота.' if state.get('connection_lost') else
            ('Требуется участие в Camoufox.' if state.get('mode')=='manual' else 'Бот сообщит, когда потребуется участие.'))
        active=self.controller.active
        self.profile_combo.setEnabled(not active)
        self.start_button.setText('Ручное оформление' if state.get('booking_handoff') else ('Продолжить' if state.get('paused') else 'Пауза') if active else 'Запустить')
        for button in (self.restart_button,self.browser_button): button.setEnabled(active)
        self.update_start()
        if state.get('booking_handoff'):self.start_button.setEnabled(False)
        if state.get('browser_retained'):
            for button in (self.start_button,  self.restart_button): button.setEnabled(False)
        self.update_state_presentation()
        self.retranslate()
        if state.get('mode')=='manual' and previous_mode!='manual':
            QAccessible.updateAccessibility(QAccessibleAnnouncementEvent(self.status_label,self.status_label.text()))

    def update_state_presentation(self):
        self.update_application_icon()
        state=self.state; mode=state.get('mode'); active=self.controller.active
        retained=state.get('browser_retained',False); lost=state.get('connection_lost',False)
        tone='danger' if lost or self.errors or state.get('failed') else 'muted' if not active and not retained else 'danger' if lost or mode=='recovery' else 'attention' if mode=='manual' else 'accent' if active and mode!='finished' else 'muted'
        for widget in (self.status_card,self.state_rail,self.status_label):
            widget.setProperty('tone',tone); widget.style().unpolish(widget); widget.style().polish(widget)
        self.status_dot.setPixmap(event_icon(theme_colors(self.preferences['dark'])[tone if tone!='accent' else 'accent_text']).pixmap(12,12))
        promote=self.restart_button if lost else self.browser_button if mode in {'manual','recovery'} or retained else self.start_button
        for button in (self.start_button,self.restart_button,self.browser_button):
            button.setProperty('primary',button is promote); button.style().unpolish(button); button.style().polish(button)
        self.browser_button.setText('Camoufox')
        self.browser_button.setAccessibleName(self.t('Показать Camoufox'))
        self.browser_button.setVisible(True)
        self.restart_button.setVisible(False)
        self.more_actions.setVisible(True)
        self.more_actions.setEnabled(not retained)
        self.restart_action.setEnabled(bool(self.profile_combo.currentData()) and not retained)
        self.attention_label.setVisible(True)
        self.attention_label.setText('Требуется участие в Camoufox' if mode=='manual' or retained else 'Проверить состояние бота' if lost else 'Для начала — «Запустить»' if not active and not state.get('mode') else 'Участие не требуется')
        self.attention_label.setProperty('tone','attention' if mode=='manual' or retained else 'muted')
        self.attention_label.style().unpolish(self.attention_label); self.attention_label.style().polish(self.attention_label)
        timer_visible=state.get('remaining_seconds') is not None and not lost and mode not in {'manual','paused','finished'}
        self.timer_label.setVisible(True); self.timer_caption.setVisible(True)
        if not timer_visible:self.timer_label.setText('—')
        for value in (self.heading,self.next_label,self.status_label):value.setToolTip(value.text())
        self.timer_caption.setText('до нового прохода' if ('новым проходом' in state.get('next_action','') or state.get('screen')=='Сит нет — новый проход') else 'до повторной попытки' if mode=='recovery' else 'до следующего действия')
        if not timer_visible:self.timer_caption.setText('')
        if lost: self.show_error('Связь с ботом потеряна. Проверить процесс бота.','connection')
        else: self.clear_error('connection')
        if retained: self.status_label.setText('Доверши в Camoufox')

    def render_logs(self):
        if not hasattr(self,'preferences'): return
        rows=self.state.get('journal',[])
        signature=(rows,self.state.get('availability',[]),self.preferences['dark'],self.preferences['language'])
        if signature == self._last_logs: return
        self._last_logs=signature
        self.timeline.setRowCount(len(rows))
        for index,row in enumerate(reversed(rows)):
            status_text,tone=STATUSES.get(row.get('status'),('Нужна проверка','attention'))
            control=row.get('event')!='visit'
            status_text=CONTROL_LABELS.get(row.get('event'),status_text) if control else status_text
            text=description(row,self.t,lambda stage: LABELS.get(stage,(stage,''))[0])
            if control and row.get('event') in {'flow_start','flow_restart'}:
                status_text=self.t(status_text)+' №'+str(row.get('pass_number',1))
            else:
                status_text=self.t(status_text)
            values=[row.get('time','')[11:23],self.t('Управление' if control else row.get('title') or LABELS.get(row.get('stage'),('',''))[0]), status_text,text]
            for col,value in enumerate(values):
                item=QTableWidgetItem(value)
                tooltip=row.get('page','') if col==1 else value
                if col==0 and row.get('ended_at'):
                    from datetime import datetime
                    duration=(datetime.fromisoformat(row['ended_at'])-datetime.fromisoformat(row['time'])).total_seconds()
                    tooltip += '\n'+self.t('Завершено')+': '+row['ended_at'][11:23]+' · '+str(round(duration,1))+' '+self.t('секунд')
                item.setToolTip(tooltip)
                colors=theme_colors(self.preferences['dark'])
                if tone in {'danger','attention'}: item.setBackground(QColor(colors[tone+'_soft']))
                if col==2:
                    item.setIcon(event_icon(colors[tone]))
                    item.setForeground(QColor(colors[tone]))
                if col==0: item.setFont(time_font())
                self.timeline.setItem(index,col,item)
        self.timeline_empty.setVisible(not rows)
        self.timeline.setVisible(bool(rows))
        rows=self.state.get('availability',[]); self.offices.setRowCount(len(rows))
        self.offices_empty.setVisible(not rows)
        self.offices.setVisible(bool(rows))
        count=sum(len(row.get('offices',[])) for row in rows)
        if len(rows)>self._availability_count: self.log_tabs.setCurrentIndex(1)
        self._availability_count=len(rows)
        self.log_tabs.setTabText(1,self.t('Доступные офисы')+(' · '+str(count) if count else ''))
        for index,row in enumerate(reversed(rows)):
            for col,value in enumerate([row.get('time','')[11:23],' · '.join(o['text'] for o in row.get('offices',[]))]):
                item=QTableWidgetItem(value); item.setToolTip(value)
                if col==0: item.setFont(time_font())
                self.offices.setItem(index,col,item)

        self.update_copy_label()

    def update_copy_label(self, *_):
        if not hasattr(self, "timeline") or not hasattr(self, "offices"):
            return
        table=self.timeline if self.log_tabs.currentIndex()==0 else self.offices
        self.copy_button.setEnabled(table.rowCount()>0)
        selected=bool(table.selectionModel().selectedRows())
        self.copy_button.setText('')
        self.copy_button.setIcon(self.control_icon('clipboard'))
        self.copy_button.setAccessibleName(self.t('Копировать выделенное' if selected else 'Копировать журнал'))
        self.copy_button.setToolTip(self.t('Копировать выделенное')+' (⌘C / Ctrl+C)' if selected else self.t('Копировать журнал'))

    def log_copy_menu(self, table, point):
        menu=QMenu(table)
        selected=menu.addAction(self.t('Копировать выделенное'))
        selected.setEnabled(bool(table.selectionModel().selectedRows()))
        selected.triggered.connect(self.copy_log)
        entire=menu.addAction(self.t('Копировать весь журнал'))
        entire.triggered.connect(lambda: self.copy_log(all_rows=True))
        menu.exec(table.viewport().mapToGlobal(point))

    def copy_log(self, _checked=False, *, all_rows=False):
        table=self.timeline if self.log_tabs.currentIndex()==0 else self.offices
        rows=['\t'.join(table.horizontalHeaderItem(c).text() for c in range(table.columnCount()))]
        selected=sorted(index.row() for index in table.selectionModel().selectedRows())
        indices=range(table.rowCount()) if all_rows or not selected else selected
        rows += ['\t'.join(table.item(r,c).text() if table.item(r,c) else '' for c in range(table.columnCount())) for r in indices]
        QApplication.clipboard().setText('\n'.join(rows))
        self.statusBar().showMessage(self.t('Журнал скопирован'),3000)

    def open_debug_folder(self):
        path=Path(self.store.settings().diagnostics_directory)
        if not path.is_absolute(): path=runtime_root()/path
        path.mkdir(parents=True,exist_ok=True,mode=0o700)
        QDesktopServices.openUrl(QUrl.fromLocalFile(str(path)))

    def refresh_error_band(self):
        from .startup_report import report_id_from_message, read_startup_report
        report_id=report_id_from_message(self.errors.get('worker',''))
        report=read_startup_report(report_id)
        self.startup_report_id=report_id if report else None
        for widget in (self.error_icon,self.error_help,self.copy_report_button,self.report_privacy,self.report_reference):
            widget.setVisible(bool(report))
        if report:
            browser=report.get('browser',{})
            missing=report.get('phase')=='browser_launch' and report.get('frozen') and not browser.get('exists') and browser.get('stat_error',{}).get('errno',2) in {2,20}
            title='Встроенный браузер не найден' if missing else 'Не удалось запустить Camoufox'
            if browser.get('stat_error',{}).get('errno') in {1,13}: title='Нет доступа к встроенному браузеру'
            if report.get('phase') in {'worker','worker_exit'}: title='Не удалось запустить поиск'
            self.error_text.setText(self.t(title))
            self.error_text.setProperty('reportTitle',True)
            self.error_icon.setPixmap(self.control_icon('alert').pixmap(20,20))
            self.error_help.setText(self.t('Скопировать отчёт и отправить автору для проверки.'))
            self.copy_report_button.setIcon(self.control_icon('check' if self.report_copy_timer.isActive() else 'clipboard'))
            self.copy_report_button.setText(self.t('Отчёт скопирован' if self.report_copy_timer.isActive() else 'Скопировать отчёт'))
            self.copy_report_button.setProperty('reportCopied',self.report_copy_timer.isActive())
            self.copy_report_button.style().unpolish(self.copy_report_button); self.copy_report_button.style().polish(self.copy_report_button)
            self.report_privacy.setText(self.t('Без данных профиля и содержимого страниц'))
            self.report_reference.setText(self.t('Отчёт')+' CEH-'+report_id)
            others=[self.t(value) for key,value in self.errors.items() if key!='worker']
            if others:self.error_text.setText(self.error_text.text()+'\n'+'\n'.join(others))
        else:
            self.error_text.setProperty('reportTitle',False)
            self.error_text.setText('\n'.join(self.t(text) for text in self.errors.values()))
        self.error_text.style().unpolish(self.error_text); self.error_text.style().polish(self.error_text)

    def copy_startup_report(self):
        from .startup_report import read_startup_report, report_clipboard_text
        report=read_startup_report(self.startup_report_id)
        if not report:
            self.refresh_error_band()
            return
        QApplication.clipboard().setText(report_clipboard_text(self.startup_report_id,report))
        self.report_copy_timer.start(4000)
        self.refresh_error_band()
        self.statusBar().showMessage(self.t('Отчёт скопирован. Вставить текст в Telegram или GitHub Issues.'),4000)

    def show_error(self, message, source='worker'):
        self.errors[source]=message
        self.update_application_icon()
        self.refresh_error_band()
        self.error_band.show()
        self.retranslate()

    def clear_error(self, source):
        self.errors.pop(source,None)
        self.update_application_icon()
        self.refresh_error_band()
        self.error_band.setVisible(bool(self.errors))

    def on_finished(self, code):
        self.profile_combo.setEnabled(True)
        self.start_button.setText('Повторить запуск' if code else 'Запустить')
        for button in (self.restart_button,self.browser_button): button.setEnabled(False)
        if code and not self.errors: self.show_error('Поиск завершился с ошибкой. Проверить диагностику сессии.')
        self.status_label.setText('Поиск остановлен' if not code else 'Поиск завершился с ошибкой')
        self.next_label.setText('Для начала — «Запустить»')
        self.state=dict(self.state,mode='finished',remaining_seconds=None,failed=bool(code))
        profile_id=getattr(self.controller,'profile_id',None) or self.profile_combo.currentData()
        self.notification_delivery.observe(profile_id,self.state,self.preferences['language'])
        self.timer_label.setText('—'); self.timer_label.hide(); self.timer_caption.hide()
        self.update_start(); self.update_state_presentation(); self.retranslate()
        if self.pending_update_install is not None:
            path=self.pending_update_install;self.pending_update_install=None
            self.launch_update(path)
        elif self.closing: self.close()

    def set_language(self, language):
        self.pending_language=language
        self.save_preferences()

    def t(self, text):
        return translate(text, getattr(self,'preferences',{}).get('language','en'))

    def retranslate(self):
        if not hasattr(self,'preferences'): return
        self.refresh_error_band()
        def update(key, current, setter):
            source, previous = self._translation_sources.get(key,(current,current))
            if current != previous: source=current
            translated=self.t(source)
            self._translation_sources[key]=(source,translated)
            setter(translated)
        # The compact monitor keeps evidence outside the visible widget tree.
        update((self.evidence,'text'),self.evidence.text(),self.evidence.setText)
        self.tabs.setIconSize(QSize(18,18))
        for index,name in enumerate(('activity','statistics','settings')):self.tabs.setTabIcon(index,self.control_icon(name))
        for object_name,icon in [('createProfile','plus'),('editProfile','pencil'),('saveProfile','check'),('cancelProfile','close'),('openDiagnostics','folder')]:
            button=self.findChild(QPushButton,object_name)
            if button:button.setIcon(self.control_icon(icon));button.setIconSize(QSize(17,17))
        self.statistics_panel.refresh()
        for widget in self.findChildren(QLabel)+self.findChildren(QPushButton):
            if widget.property('notificationTestButton'):widget.refresh_text();continue
            if self.statistics_panel.isAncestorOf(widget) or widget.property('notificationData'):continue
            if widget.property('notificationDisclosure'):widget.refresh_text();continue
            source=widget.property('notificationSource')
            if source:widget.setText(self.t(source))
            else:update((widget,'text'),widget.text(),widget.setText)
        for widget in self.findChildren(Toggle):
            update((widget,'accessible'),widget.accessibleName(),widget.setAccessibleName)
        for tabs in (self.tabs,self.log_tabs,self.profile_tabs):
            for index in range(tabs.count()):
                update((tabs,index),tabs.tabText(index),lambda text,i=index,t=tabs:t.setTabText(i,text))
        for widget in self.fields.values():
            update((widget,'accessible'),widget.accessibleName(),widget.setAccessibleName)
        for widget in self.fields.values():
            if isinstance(widget,QLineEdit):
                update((widget,'placeholder'),widget.placeholderText(),widget.setPlaceholderText)
        for name in ('access_mode','fill_mode','document_type'):
            combo=self.fields[name]; combo.blockSignals(True)
            for index in range(combo.count()):
                update((combo,index),combo.itemText(index),lambda text,i=index,c=combo:c.setItemText(i,text))
            combo.blockSignals(False)
        for table in (self.timeline,self.offices):
            for index in range(table.columnCount()):
                item=table.horizontalHeaderItem(index)
                update((table,index),item.text(),item.setText)
        language=self.preferences['language']
        self.success_action.setText(self.t("Сита получена"))
        self.restart_action.setText(self.t('Перезапустить сессию' if self.controller.active else 'Сбросить сессию'))
        self.more_actions.setToolTip(self.t('Другие действия'))
        self.more_actions.setAccessibleName(self.t('Другие действия'))
        self.start_button.setIcon(self.control_icon('play' if not self.controller.active or self.state.get('paused') else 'pause'))
        self.start_button.setIconSize(QSize(16,16))
        self.language_button.setText(language.upper())
        self.language_button.setIcon(QIcon())
        self.language_button.setToolTip(self.t('Язык интерфейса')+' · '+LANGUAGES[language])
        self.language_button.setAccessibleName(self.t('Язык интерфейса'))
        for code,action in self.language_actions.items(): action.setChecked(code==language)
        self.dark_toggle.setText('')
        self.dark_toggle.setIcon(self.control_icon('sun' if self.preferences['dark'] else 'moon'))
        theme_action='Светлая тема' if self.preferences['dark'] else 'Тёмная тема'
        self.dark_toggle.setToolTip(self.t(theme_action))
        self.dark_toggle.setAccessibleName(self.t(theme_action))
        self.settings_theme.blockSignals(True); self.settings_theme.setChecked(self.preferences['dark']); self.settings_theme.blockSignals(False)
        self.settings_language.blockSignals(True); self.settings_language.setCurrentIndex(self.settings_language.findData(language)); self.settings_language.blockSignals(False)
        self.sound_button.blockSignals(True); self.sound_button.setChecked(self.preferences['sound']); self.sound_button.blockSignals(False)
        sound_action='Выключить звук' if self.preferences['sound'] else 'Включить звук'
        self.sound_button.setIcon(self.control_icon('bell' if self.preferences['sound'] else 'bell_off'))
        self.sound_button.setToolTip(self.t(sound_action)); self.sound_button.setAccessibleName(self.t(sound_action))
        self.browser_button.setIcon(self.control_icon('external'))
        self.service_icon.setPixmap(self.control_icon('clipboard').pixmap(QSize(18,18)))
        self.about_button.setIcon(self.control_icon('info'))
        self.about_button.setToolTip(self.t('О приложении'))
        self.about_button.setAccessibleName(self.t('О приложении'))
        field_icons={'label':'user','province':'pin','procedure':'clipboard','access_mode':'key','document_type':'id','document_number':'id',
                     'full_name':'user','nationality':'globe','phone':'phone','email':'mail'}
        for key,name in field_icons.items():self.profile_field_boxes[key].icon.setPixmap(self.control_icon(name).pixmap(QSize(18,18)))
        for combo,box,caption,icon,choices in (
            (self.applicant_no_slots_combo,self.applicant_no_slots_box,'На форме заявителя','user',('Ждать при «нет сит»','Вводить и продолжать')),
            (self.no_slots_mode_combo,self.no_slots_mode_box,'После ответа «нет сит»','restart',('Обновлять результат','Через Salir'))):
            box.caption.setText(self.t(caption));box.icon.setPixmap(self.control_icon(icon).pixmap(QSize(18,18)))
            combo.setAccessibleName(self.t(caption))
            for index,title in enumerate(choices):combo.setItemText(index,self.t(title))
        self.update_search_behavior()
        for key,box in self.timing_field_boxes.items():
            box.help.setToolTip(self.t(box.help_text))
            box.icon.setPixmap(self.control_icon('bell' if key=='alert_seconds' else 'globe' if key=='reload_seconds' else 'info').pixmap(QSize(17,17)))
            box.help.setPixmap(self.control_icon('info').pixmap(QSize(15,15)))
        for value,button in self.identity_buttons.items():button.setIcon(self.control_icon('copy' if value=='copy' else 'pencil'))
        self.notifications_panel.retranslate()
        for widget in self.findChildren(QToolButton)+self.findChildren(QPushButton):
            icon=widget.property('notificationIcon')
            if icon:widget.setIcon(self.control_icon(icon))
            title=widget.property('notificationTooltip')
            if title:widget.setToolTip(self.t(title));widget.setAccessibleName(self.t(title))
        for widget in self.findChildren(QLabel):
            icon=widget.property('notificationFieldIcon')
            if icon:widget.setPixmap(self.control_icon(icon).pixmap(QSize(18,18)))
        self.update_identity_fields()
        self.render_logs()
        self.update_copy_label()
        for image,icon in self.empty_icon_labels:image.setPixmap(self.control_icon(icon).pixmap(QSize(28,28)))
        # Compact controls keep their width across ordinary work states.
        self.start_button.setFixedWidth(max(138,max(self.start_button.fontMetrics().horizontalAdvance(self.t(text)) for text in ('Запустить','Пауза','Продолжить'))+40,
                                           self.start_button.fontMetrics().horizontalAdvance(self.t('Ручное оформление'))+40 if self.state.get('booking_handoff') else 0))
        self.browser_button.setFixedWidth(120)
        for button in (self.start_button,self.browser_button):button.setFixedHeight(36)
        self.update_editor_height()

    def update_editor_height(self, *_):
        # Each tab owns a scroll area; never force content height onto a dialog.
        self.profile_tabs.setMinimumHeight(180)

    def show_about(self):
        from ..version import VERSION
        dialog=QDialog(self); self.about_dialog=dialog
        dialog.setWindowTitle(self.t('О приложении'));dialog.setFixedWidth(560)
        layout=QVBoxLayout(dialog);layout.setContentsMargins(24,24,24,20);layout.setSpacing(16)
        identity=QHBoxLayout();identity.setSpacing(14);app_icon=QLabel();app_icon.setObjectName('aboutAppIcon');app_icon.setFixedSize(58,58);app_icon.setPixmap(QIcon(str(Path(__file__).parent/'assets/icon.svg')).pixmap(58,58));identity.addWidget(app_icon)
        words=QVBoxLayout();words.setSpacing(4);words.addWidget(label(display_name(),name='heading'))
        from .easter_egg import VersionLabel,AppointmentChase
        version=VersionLabel(self.t('Версия')+' '+VERSION);version.setObjectName('aboutVersion');version.setProperty('muted',True)
        def open_chase():
            game=AppointmentChase(dialog,self.t,language=self.preferences['language']);self.appointment_chase=game;game.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose);game.show()
        version.activated.connect(open_chase);words.addWidget(version);identity.addLayout(words,1);layout.addLayout(identity)
        layout.addWidget(label(self.t('Помощник для записи на отпечатки пальцев в Extranjería. Заполняет формы, проверяет доступные ситы и сообщает, когда требуется участие.')))
        updates=QFrame();updates.setObjectName('aboutUpdates');updates_layout=QHBoxLayout(updates);updates_layout.setContentsMargins(14,12,14,12);updates_layout.setSpacing(12);shield=QLabel();shield.setObjectName('aboutUpdateShield');shield.setFixedSize(22,22);shield.hide();updates_layout.addWidget(shield);status=QVBoxLayout();status.setSpacing(4);status.addWidget(label(self.t('Обновления'),name='sectionHeading'))
        latest=label(self.t('Последняя версия')+': '+self.t('Ещё не проверена'),muted=True);latest.setObjectName('latestVersion');status.addWidget(latest);updates_layout.addLayout(status,1)
        check=QPushButton(self.t('Проверить'));check.setObjectName('checkUpdates');updates_layout.addWidget(check);layout.addWidget(updates)
        repository='https://github.com/itwormz/extranjeria-cita-helper';readme={'ru':'README.ru.md','es':'README.es.md'}.get(self.preferences['language'],'README.md');links=QHBoxLayout();links.setSpacing(8)
        for name,title,url,icon,stretch in [('githubLink','GitHub',repository,'github',8),('readmeLink','Как пользоваться',repository+'/blob/main/'+readme,'book',11),('issuesLink','Сообщить о проблеме',repository+'/issues/new','bug',15)]:
            link=QPushButton();link.setObjectName(name);link.setMinimumHeight(18);link.setSizePolicy(QSizePolicy.Policy.Expanding,QSizePolicy.Policy.Fixed);link.setText(self.t(title));link.setIcon(self.control_icon(icon));link.setIconSize(QSize(18,18));link.setProperty('targetUrl',url);link.setToolTip(url);link.setAccessibleName(self.t(title));link.clicked.connect(lambda checked=False,target=url:QDesktopServices.openUrl(QUrl(target)));links.addWidget(link,stretch)
        layout.addLayout(links)
        download=QPushButton(self.t('Скачать обновление'));download.setObjectName('downloadUpdate');layout.addWidget(download)
        install=QPushButton(self.t('Установить обновление'));install.setObjectName('installUpdate');layout.addWidget(install)
        cancel=QPushButton(self.t('Отменить скачивание'));layout.addWidget(cancel)
        hint=label(self.t('Перед установкой текущая сессия будет завершена. Профили и настройки сохранятся.'),muted=True);hint.setObjectName('aboutInstallHint');layout.addWidget(hint)
        footer=QHBoxLayout();local=label(self.t('Независимый проект')+'\n'+self.t('Данные хранятся локально'),muted=True);footer.addWidget(local,1);close=QPushButton(self.t('Закрыть'));close.clicked.connect(dialog.close);footer.addWidget(close);layout.addLayout(footer)
        self.update_widgets=(dialog,latest,check,download,install,cancel)
        check.clicked.connect(self.check_updates)
        download.clicked.connect(lambda:self.update_service.download(self.update_release) if self.update_release else None)
        install.clicked.connect(self.install_update)
        cancel.clicked.connect(self.update_service.cancel)
        self.render_updates()
        dialog.ensurePolished();dialog.layout().activate()
        for text in dialog.findChildren(QLabel):
            if text.wordWrap(): text.setMinimumHeight(max(0,text.heightForWidth(text.width())))
        dialog.adjustSize()
        dialog.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose); dialog.show()

    def automatic_update_check(self):
        if self.preferences.get('automatic_updates',True) and not self.update_service.busy and self.update_service.due():
            self.check_updates()

    def check_updates(self):
        if self.update_service.check():
            self.update_message='Проверяем обновления…';self.render_updates()

    def update_checked(self, release):
        if self.update_path and (release is None or self.update_path.name!=release.name):
            self.update_path=None
        self.update_release=release
        self.update_message=('Доступна новая версия' if release else 'Установлена актуальная версия')
        if release:
            self.footer.setText(self.t('Доступна новая версия')+': '+release.version+' — '+self.t('О приложении'))
        self.render_updates()

    def update_downloaded(self,path):
        self.update_path=path;self.update_message='Обновление скачано и проверено';self.render_updates()

    def update_failed(self,reason):
        self.update_install_requested=False
        if reason=='provenance':self.update_path=None
        self.update_message={'provenance':'Не удалось подтвердить происхождение обновления. Установка заблокирована.',
                             'no_release':'Релиз ещё не опубликован','cancelled':'Скачивание отменено',
                             'network':'Не удалось проверить обновления. Повторить попытку позже.'}.get(reason,'Не удалось проверить или скачать обновление. Повторить попытку позже.')
        self.render_updates()

    def update_progress(self,percent):
        self.update_message=self.t('Скачивание обновления')+f': {percent}%';self.render_updates()

    def update_busy(self,busy):
        self.render_updates()
        if not busy and self.closing:self.close()

    def render_updates(self):
        widgets=getattr(self,'update_widgets',None)
        if not widgets:return
        from shiboken6 import isValid
        dialog,latest,check,download,install,cancel=widgets
        if not isValid(dialog):return
        version=' · '+self.update_release.version if self.update_release else ''
        latest.setText(self.t(self.update_message)+version)
        blocked=self.update_message=='Не удалось подтвердить происхождение обновления. Установка заблокирована.'
        tone='danger' if blocked else 'normal'
        frame=dialog.findChild(QFrame,'aboutUpdates')
        for widget in (frame,latest):
            widget.setProperty('tone',tone);widget.style().unpolish(widget);widget.style().polish(widget)
        shield=dialog.findChild(QLabel,'aboutUpdateShield')
        shield.setPixmap(self.control_icon('shield-alert').pixmap(QSize(22,22)));shield.setVisible(blocked)
        busy=self.update_service.busy
        check.setEnabled(not busy)
        download.setEnabled(bool(self.update_release) and not busy)
        download.setVisible(bool(self.update_release) and not self.update_path)
        install.setVisible(bool(self.update_path));install.setEnabled(not busy)
        cancel.setVisible(busy and bool(self.update_release))
        hint=dialog.findChild(QLabel,'aboutInstallHint')
        if hint:hint.setVisible(bool(self.update_release or self.update_path))
        if dialog.isVisible():
            latest.setMinimumHeight(0);dialog.layout().activate()
            latest.setMinimumHeight(max(0,latest.heightForWidth(latest.width())));dialog.adjustSize()

    def install_update(self):
        if self.update_path and not self.update_service.busy:
            self.update_install_requested=True;self.update_service.verify(self.update_path)

    def update_verified(self,path):
        if not self.update_install_requested:return
        self.update_install_requested=False
        if self.controller.active:
            self.pending_update_install=path;self.controller.shutdown(close_browser=True)
        else:self.launch_update(path)

    def launch_update(self,path):
        import subprocess
        try:
            from .paths import portable_mode
            if portable_mode():
                QDesktopServices.openUrl(QUrl.fromLocalFile(str(Path(path).parent)))
            elif sys.platform=='win32':
                executable=Path(__import__('os').environ['SystemRoot'])/'System32/msiexec.exe'
                subprocess.Popen([str(executable),'/i',str(path)])
                self.close()
            elif sys.platform=='darwin':
                subprocess.Popen(['/usr/bin/open',str(path)]);self.close()
            else:
                Path(path).chmod(Path(path).stat().st_mode | 0o100)
                QDesktopServices.openUrl(QUrl.fromLocalFile(str(Path(path).parent)))
                self.update_message='Пакет готов к установке';self.render_updates()
        except OSError:
            self.update_message='Не удалось открыть установщик';self.render_updates()

    def control_icon(self, name):
        paths = {
            'zoom_in':'<circle cx="10" cy="10" r="7"/><path d="m15 15 6 6M6 10h8M10 6v8"/>',
            'zoom_out':'<circle cx="10" cy="10" r="7"/><path d="m15 15 6 6M6 10h8"/>',
            'alert':'<circle cx="12" cy="12" r="9"/><path d="M12 6v7m0 3v1"/>',
            'book':'<path d="M12 5v16M3 4h5l4 2 4-2h5v16h-5l-4 2-4-2H3Z"/>',
            'bug':'<rect x="7" y="7" width="10" height="14" rx="5"/><path d="m8 3 4 4 4-4M2 10h5m10 0h5M2 16h5m10 0h5M12 7v14"/>',
            'test':'<path d="M9 3h6M10 3v7L4 20a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1l-6-10V3M8 14h8"/>',
            'trash':'<path d="M3 6h18M9 6V3h6v3M6 6l1 15h10l1-15M10 10v7M14 10v7"/>',
            'send':'<path d="m22 2-7 20-4-9L2 9ZM22 2 11 13"/>',
            'plus':'<path d="M12 4v16M4 12h16"/>',
            'close':'<path d="m5 5 14 14M5 19 19 5"/>',
            'folder':'<path d="M3 5h7l2 3h9v13H3Z"/>',
            'wrench':'<path d="m14 6 4 4 4-4a7 7 0 0 1-9 9L5 23l-4-4 8-8a7 7 0 0 1 9-9Z"/>',
            'text':'<path d="M3 4h18M12 4v16M8 20h8"/>',
            'back':'<path d="m10 5-7 7 7 7M3 12h18"/>',
            'activity':'<path d="M2 12h4l3-8 6 16 3-8h4"/>',
            'settings':'<polygon points="9.93,4.27 10.61,2.10 13.39,2.10 14.07,4.27 16.00,5.07 18.02,4.01 19.99,5.98 18.93,8.00 19.73,9.93 21.90,10.61 21.90,13.39 19.73,14.07 18.93,16.00 19.99,18.02 18.02,19.99 16.00,18.93 14.07,19.73 13.39,21.90 10.61,21.90 9.93,19.73 8.00,18.93 5.98,19.99 4.01,18.02 5.07,16.00 4.27,14.07 2.10,13.39 2.10,10.61 4.27,9.93 5.07,8.00 4.01,5.98 5.98,4.01 8.00,5.07"/><circle cx="12" cy="12" r="3"/>',
            'award':'<circle cx="12" cy="8" r="6"/><path d="m8 13-2 9 6-3 6 3-2-9"/>',
            'calendar_days':'<rect x="3" y="5" width="18" height="16" rx="2"/><path d="M7 2v6M17 2v6M3 10h18M7 14h.2M12 14h.2M17 14h.2M7 18h.2M12 18h.2"/>',
            'eye':'<path d="M2 12s4-8 10-8 10 8 10 8-4 8-10 8S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/>',
            'restart':'<path d="M3 10a9 9 0 1 1 2 9M3 3v7h7"/>',
            'share':'<circle cx="18" cy="5" r="3"/><circle cx="6" cy="12" r="3"/><circle cx="18" cy="19" r="3"/><path d="m9 10 6-4M9 14l6 4"/>',
            'lock':'<rect x="4" y="10" width="16" height="12" rx="2"/><path d="M8 10V6a4 4 0 0 1 8 0v4M12 15v3"/>',
            'statistics':'<path d="M4 20V12M12 20V4M20 20V8"/>',
            'search':'<circle cx="10" cy="10" r="7"/><path d="m15 15 6 6"/>',
            'clock':'<circle cx="12" cy="12" r="9"/><path d="M12 6v6l4 2"/>',
            'calendar':'<rect x="3" y="5" width="18" height="16" rx="2"/><path d="M7 2v6M17 2v6M3 10h18m-14 6 3 3 6-6"/>',
            'list':'<path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01"/>',
            'office':'<rect x="5" y="3" width="14" height="18" rx="1"/><path d="M9 7h1m4 0h1M9 11h1m4 0h1M9 15h1m4 0h1M10 21v-3h4v3"/>',
            'server':'<rect x="3" y="3" width="18" height="7" rx="1"/><rect x="3" y="14" width="18" height="7" rx="1"/><path d="M7 6h.2M7 17h.2"/>',
            'shield-alert':'<path d="m12 2 9 4v6c0 5-9 10-9 10S3 17 3 12V6Z"/><path d="M12 7v6m0 3v1"/>',
            'shield':'<path d="m12 2 9 4v6c0 5-9 10-9 10S3 17 3 12V6Z"/>',
            'check':'<path d="m4 12 5 5L20 5"/>',
            'clipboard':'<rect x="5" y="5" width="14" height="16" rx="2"/><rect x="9" y="2" width="6" height="5" rx="1"/>',
            'external':'<path d="M14 3h7v7M21 3L10 14M10 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-5"/>',
            'pointer':'<path d="M5 3l14 10-7 1-3 7z"/><path d="M12 14l4 7"/>',
            'file_text':'<path d="M14 2H5v20h14V7z"/><path d="M14 2v5h5M8 12h8M8 16h6"/>',
            'copy':'<rect x="8" y="8" width="13" height="13" rx="2"/><path d="M16 8V3H3v13h5"/>',
            'download':'<path d="M12 2v13m-5-5 5 5 5-5M3 17v4h18v-4"/>',
            'credit_card':'<rect x="2" y="4" width="20" height="16" rx="3"/><path d="M2 9h20M6 15h5"/>',
            'bitcoin':'<circle cx="12" cy="12" r="10"/><path d="M9 7h5a3 3 0 0 1 0 6H9m0-1h5a3 3 0 0 1 0 6H9V6m2-2v3m3-3v3m-3 11v3m3-3v3"/>',
            'user':'<circle cx="12" cy="8" r="4"/><path d="M4 21v-2a8 8 0 0 1 16 0v2"/>',
            'pin':'<path d="M20 10c0 6-8 12-8 12S4 16 4 10a8 8 0 0 1 16 0Z"/><circle cx="12" cy="10" r="2.5"/>',
            'key':'<circle cx="8" cy="8" r="5"/><path d="m12 12 9 9M17 17l3-3M14 14l3-3"/>',
            'id':'<rect x="2" y="5" width="20" height="14" rx="2"/><circle cx="8" cy="10" r="2"/><path d="M5 16h6M14 10h5M14 14h5"/>',
            'phone':'<path d="M5 3h4l2 5-3 2a16 16 0 0 0 6 6l2-3 5 2v4c0 2-2 3-4 2A23 23 0 0 1 3 7c-1-2 0-4 2-4Z"/>',
            'mail':'<rect x="2" y="4" width="20" height="16" rx="2"/><path d="m2 6 10 7 10-7"/>',
            'pencil':'<path d="m16 3 5 5L8 21H3v-5ZM13 6l5 5"/>',
            'pause':'<path d="M8 5v14M16 5v14"/>',
            'play':'<path d="m8 4 12 8-12 8Z"/>',
            'github': '<path d="M9 19c-4 1-4-2-6-2m12 5v-4a3.5 3.5 0 0 0-1-2.7c3.3-.4 6.7-1.6 6.7-7.3a5.7 5.7 0 0 0-1.5-4 5.3 5.3 0 0 0-.1-4s-1.2-.4-4.1 1.5a14 14 0 0 0-7.4 0C5.7 1.6 4.5 2 4.5 2a5.3 5.3 0 0 0-.1 4A5.7 5.7 0 0 0 3 10c0 5.7 3.4 6.9 6.7 7.3A3.5 3.5 0 0 0 9 20v2"/>',
            'bell_off':'<path d="m3 3 18 18M6 8c0 7-3 7-3 9h14M18 12V8a6 6 0 0 0-8-5M10 21h4"/>',
            'bell': '<path d="M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9M10 21h4"/>',
            'info': '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7v.2"/>',
            'globe': '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c5 5 5 13 0 18-5-5-5-13 0-18Z"/>',
            'moon': '<path d="M20 14a8.5 8.5 0 0 1-10-10A8.5 8.5 0 1 0 20 14Z"/>',
            'sun': '<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5"/>',
        }
        color = theme_colors(self.preferences['dark'])['danger' if name in {'alert','shield-alert'} else 'text']
        svg = f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="{color}" stroke-width="1.6" stroke-linecap="round">{paths[name]}</svg>'
        if name=='github':
            asset=Path(__file__).parent/'assets/social/github.svg'
            if asset.exists():svg=asset.read_text().replace('<svg ',f'<svg fill="{color}" ',1)
        pixmap = QPixmap(36, 36)
        pixmap.fill(Qt.GlobalColor.transparent)
        pixmap.setDevicePixelRatio(2)
        painter = QPainter(pixmap)
        QSvgRenderer(QByteArray(svg.encode())).render(painter, QRectF(0, 0, 18, 18))
        painter.end()
        return QIcon(pixmap)

    def eventFilter(self,watched,event):
        if event.type()==QEvent.Type.MouseButtonPress and isinstance(watched,QWidget) and (watched is self or self.isAncestorOf(watched)):
            keep=(self.timeline,self.offices,self.copy_button)
            if not any(watched is widget or widget.isAncestorOf(watched) for widget in keep):
                for table in (self.timeline,self.offices):
                    if table.selectedItems() or table.currentIndex().isValid():
                        table.clear_log_selection()
        return super().eventFilter(watched,event)

    def closeEvent(self, event):
        self.notification_delivery.stop()
        if self.update_service.busy:
            self.closing=True;self.update_service.cancel()
            if self.controller.active:self.controller.shutdown(close_browser=True)
            event.ignore();return
        if self.controller.active:
            self.closing=True; self.controller.shutdown(close_browser=True); event.ignore()
        else: event.accept()


def run_desktop():
    from ..main import _paths, ROOT
    load_dotenv(ROOT/'.env')
    path,catalog,_=_paths()
    if sys.platform == 'win32':
        import ctypes
        ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID('itwormz.ExtranjeriaCitaHelper')
    app=QApplication.instance() or QApplication(sys.argv[:1])
    app.setApplicationName(display_name())
    app.setApplicationDisplayName(display_name())
    app.setStyle('Fusion')
    load_fonts(app)
    store=ProfileStore(path,catalog); controller=DesktopController(store)
    window=DesktopWindow(store,controller); window.show()
    QTimer.singleShot(2000,window.automatic_update_check)
    update_timer=QTimer(window);update_timer.setInterval(3600_000)
    update_timer.timeout.connect(window.automatic_update_check);update_timer.start()
    return app.exec()
