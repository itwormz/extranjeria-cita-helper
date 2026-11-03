"""Per-profile observations and locally confirmed appointment achievements."""
import logging
from ..logging_utils import log_event
from datetime import datetime, timezone
from pathlib import Path

from PySide6.QtCore import Qt, QDate, QTime, QTimer, QUrl, QSize, Signal, QEventLoop, QRunnable, QThreadPool
from PySide6.QtGui import QDesktopServices, QPainter, QColor, QFont, QIcon, QPixmap
from PySide6.QtWidgets import (QWidget, QLabel, QPushButton, QToolButton, QComboBox,
    QVBoxLayout, QHBoxLayout, QGridLayout, QStackedWidget, QFrame, QDialog,
    QDialogButtonBox, QSizePolicy, QDateEdit, QTimeEdit, QAbstractSpinBox, QLineEdit, QFileDialog, QApplication, QMenu, QLayout)
from .theme import theme_colors
from .page_stack import CurrentPageStack
from .statistics import ProfileStatistics
from .statistics_history import StatisticsHistory, MADRID, stamp, history_revision, read_history
from .achievements import AchievementStore
from .achievement_exports import public_post, write_calendar, render_card, share_url, public_duration

log=logging.getLogger(__name__)

METRICS=(('checks','Проверок','search'),('active_seconds','Активный поиск','clock'),
    ('unique_offices','Разных офисов','pin'),('booking_screens','Экранов ситы','calendar'),
    ('offices_seen','Появлений офисов','office'),('server_errors','Ошибок сервера','server'),
    ('rate_limited','HTTP 429','clock'),('blocked','Ограничения доступа','shield'),
    ('flow_passes','Повторные проходы','restart'),('actions_performed','Действий выполнено','pointer'),('pages_processed','Страниц обработано','file_text'),('first_available_seconds','До первой доступности','clock'))


def duration(seconds,t):
    if seconds is None:return '—'
    minutes=int(seconds)//60
    hours,minutes=divmod(minutes,60)
    return f'{hours} {t("ч")} {minutes} {t("мин")}' if hours else f'{minutes} {t("мин")}'


class FoodSticker(QWidget):
    def __init__(self):
        super().__init__();self.setFixedSize(66,72);self.setAccessibleName('Will code for food')
    def paintEvent(self,event):
        p=QPainter(self);p.setRenderHint(QPainter.RenderHint.Antialiasing)
        p.setPen(self.palette().text().color());p.drawEllipse(27,3,11,11)
        p.drawLine(32,15,32,44);p.drawLine(32,24,9,36);p.drawLine(32,24,55,36)
        p.drawLine(32,44,22,68);p.drawLine(32,44,43,68)
        p.setBrush(QColor('#D0B48F'));p.setPen(QColor('#80694C'));p.drawRoundedRect(3,30,59,28,2,2)
        p.setPen(QColor('#30271E'));font=p.font();font.setPointSize(8);font.setBold(True);p.setFont(font)
        p.drawText(3,30,59,28,Qt.AlignmentFlag.AlignCenter,'Will code\nfor food');p.end()


class _Load(QRunnable):
    def __init__(self, work):
        super().__init__();self.work=work
    def run(self):
        self.work()


def _file_revision(path):
    try:stat=path.stat()
    except FileNotFoundError:return None
    return (stat.st_mtime_ns,stat.st_size)


# Distinct from a missing file, whose revision is None.
_UNSTABLE_REVISION=object()


def load_statistics_view(statistics, achievements, profile_id, period, custom_range, scope):
    """Read totals, achievements and history without touching widgets."""
    try:
        component='totals';statistics_before=history_revision(statistics);data=statistics.get(profile_id,period=period)
        component='achievements';achievement_before=_file_revision(achievements.path);records=achievements.list(profile_id);achievement_after=_file_revision(achievements.path)
        component='history';loaded=read_history(statistics,profile_id,period,custom_range);statistics_after=history_revision(statistics)
        signature=(scope,statistics_after if statistics_before==statistics_after else _UNSTABLE_REVISION,achievement_after if achievement_before==achievement_after else _UNSTABLE_REVISION)
        return ('ok',data,records,loaded,signature,profile_id,period,custom_range)
    except (OSError,ValueError) as exc:
        return ('err',component,exc)


class StatisticsPanel(QWidget):
    loaded=Signal(int, object)
    def __init__(self,host):
        super().__init__();self.setObjectName("statisticsPanel");self.host=host;self.profiles=[];self.current_achievement=None;self._signature=None;self._statistics_read_failure=None;self._statistics_read_message=None
        self._snapshot=None;self._render_key=None;self._history_visual=None;self._generation=0;self._inflight_token=None;self._waiting=False;self._again=False
        self._jobs=QThreadPool(self);self._jobs.setMaxThreadCount(1);self.loaded.connect(self._on_loaded)
        data=host.store.path.parent/'data';self.statistics=ProfileStatistics(data/'profile-statistics.json')
        self.achievements=AchievementStore(data/'achievements.json')
        self._text_bindings=[];self._icon_bindings=[];self._payment_buttons=[]
        layout=QVBoxLayout(self);layout.setContentsMargins(24,24,24,24);layout.setSpacing(24)
        header=QHBoxLayout();self.heading_icon=self.icon_label('statistics');header.addWidget(self.heading_icon);self.title=QLabel();self.title.setObjectName('statisticsTitle');header.addWidget(self.title,1);self.back_button=self.button('К статистике','back',self.show_statistics);header.insertWidget(0,self.back_button)
        self.profile_combo=QComboBox();self.profile_combo.setMinimumWidth(180);self.profile_combo.setAccessibleName('Профиль для статистики');self.profile_combo.currentIndexChanged.connect(self.profile_selected);header.addWidget(self.icon_label('user'));header.addWidget(self.profile_combo)
        self.more=QToolButton();self.more.setText('⋯');self.more.setPopupMode(QToolButton.ToolButtonPopupMode.InstantPopup)
        menu=QMenu(self.more);self.edit_action=menu.addAction('Исправить данные записи');self.edit_action.triggered.connect(self.edit_appointment);self.more.setMenu(menu);header.addWidget(self.more)
        layout.addLayout(header);self.stack=CurrentPageStack();self.stack.layout().setSizeConstraint(QLayout.SizeConstraint.SetNoConstraint);self.stack.currentChanged.connect(self.fit_current_page);layout.addWidget(self.stack)
        self.stats_page=QWidget();self.stats_layout=QVBoxLayout(self.stats_page);self.stats_layout.setContentsMargins(0,0,0,0);self.stats_layout.setSpacing(16)
        self.stats_hint=QLabel();self.stats_hint.setProperty('muted',True);self.stats_layout.addWidget(self.stats_hint);self.stats_hint.hide()
        period_row=QHBoxLayout();self.today_button=self.button('Сегодня',None,lambda:self.set_period('today'));self.all_button=self.button('За всё время',None,lambda:self.set_period('all'));self.period='all'
        for control in (self.today_button,self.all_button):control.setCheckable(True);control.setObjectName('statisticsPeriod');period_row.addWidget(control)
        period_row.addStretch();self.stats_layout.addLayout(period_row)
        self.statistics_warning=self.text('Не удалось сохранить часть статистики. Поиск продолжается; в истории возможны пропуски.');self.statistics_warning.setObjectName('statisticsWarning');self.stats_layout.addWidget(self.statistics_warning);self.statistics_warning.hide()
        self.stat_values=self.statistics_overview(self.stats_layout)
        budget_row = QHBoxLayout()
        self.budget_icon = self.icon_label("shield")
        self.budget_icon.hide()
        budget_row.addWidget(self.budget_icon)
        self.budget_label = QLabel()
        self.budget_label.setWordWrap(True)
        self.budget_label.setTextFormat(Qt.TextFormat.RichText)
        self.budget_label.linkActivated.connect(self.show_budget_history)
        self.budget_label.setMinimumWidth(0)
        budget_row.addWidget(self.budget_label, 1)
        self.budget_menu = QToolButton()
        self.budget_menu.setText('⋯')
        self.budget_menu.setPopupMode(QToolButton.ToolButtonPopupMode.InstantPopup)
        menu = QMenu(self.budget_menu)
        self.budget_action = menu.addAction('Отключить распределение на сегодня')
        self.budget_action.triggered.connect(self.toggle_budget_distribution)
        self.budget_history_action = menu.addAction(self.tr_text('История возможных ограничений'))
        self.budget_history_action.triggered.connect(self.show_budget_history)
        self.budget_state = {}
        self.budget_menu.setMenu(menu)
        budget_row.addWidget(self.budget_menu)
        self.budget_label.hide()
        self.budget_menu.hide()
        self.stats_layout.addLayout(budget_row)
        self.budget_disabled = False
        first_separator=QFrame();first_separator.setObjectName('journalSeparator');first_separator.setFixedHeight(1);self.stats_layout.addWidget(first_separator)
        first_grid=QGridLayout();first_grid.setHorizontalSpacing(12);first_grid.setVerticalSpacing(10)
        self.first_caption=self.text('Первый офис',muted=True);self.first_booking_caption=self.text('Первый выбор даты',muted=True)
        self.first_value=QLabel('—');self.first_booking_value=QLabel('—')
        for row,(icon,caption,value) in enumerate((('clock',self.first_caption,self.first_value),('calendar',self.first_booking_caption,self.first_booking_value))):
            caption.setMaximumWidth(190);value.setWordWrap(True)
            first_grid.addWidget(self.icon_label(icon),row,0,Qt.AlignmentFlag.AlignTop);first_grid.addWidget(caption,row,1);first_grid.addWidget(value,row,2)
        first_grid.setColumnMinimumWidth(1,190);first_grid.setColumnStretch(2,1);self.stats_layout.addLayout(first_grid)
        self.success_note=self.text('После оформления ситы можно сохранить данные записи, добавить её в календарь и поделиться статистикой поиска.',muted=True)
        actions=QHBoxLayout();actions.setSpacing(12);self.success_button=self.button('Сита получена','check',self.confirm_success,primary=True);self.success_button.setObjectName('statisticsSuccess');self.success_button.setFixedHeight(38);self.success_button.setSizePolicy(QSizePolicy.Policy.Maximum,QSizePolicy.Policy.Fixed);actions.addWidget(self.success_button)
        self.achievement_button=self.button('Достижения профиля','calendar',self.show_achievement);actions.addWidget(self.achievement_button);actions.addStretch();self.stats_layout.addLayout(actions)
        self.stats_layout.addWidget(self.success_note)
        self.history_panel=StatisticsHistory(host,self.statistics);self.stats_layout.addWidget(self.history_panel)
        self.stats_layout.addStretch();self.stack.addWidget(self.stats_page)
        self.achievement_page=QWidget();body=QVBoxLayout(self.achievement_page);body.setContentsMargins(0,0,0,0);body.setSpacing(12)
        card=QFrame();card.setObjectName('achievementCard');card_layout=QVBoxLayout(card);card_layout.setContentsMargins(20,16,20,16);card_layout.setSpacing(14)
        heading=QHBoxLayout();heading.addWidget(self.icon_label('calendar'));obtained=self.text('Сита получена');obtained.setObjectName('statisticsTitle');heading.addWidget(obtained,1);card_layout.addLayout(heading)
        self.achievement_duration=QLabel();self.achievement_duration.setObjectName('achievementDuration');card_layout.addWidget(self.achievement_duration)
        self.achievement_values=self.metric_grid(card_layout,include_first=True);credit=QLabel('Cita Extranjeria Helper · github.com/itwormz/extranjeria-cita-helper');credit.setProperty('muted',True);credit.setWordWrap(True);card_layout.addWidget(credit);body.addWidget(card)
        calendar_header=QHBoxLayout();calendar_header.addWidget(self.section_heading('Данные записи','calendar_days'),1);calendar_header.addWidget(self.button('Добавить в календарь','calendar',self.export_calendar,primary=True));body.addLayout(calendar_header)
        private_grid=QGridLayout();private_grid.setHorizontalSpacing(24);self.private_fields={}
        for column,(key,title) in enumerate([('date','Дата и время'),('office','Офис и адрес'),('confirmation','Номер подтверждения')]):
            box=QWidget();cell=QVBoxLayout(box);cell.setContentsMargins(0,0,0,0);cell.setSpacing(5);cell.addWidget(self.text(title,muted=True));value=QLabel('—');value.setTextFormat(Qt.TextFormat.PlainText);value.setWordWrap(True);value.setObjectName('appointmentValue');value.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse | Qt.TextInteractionFlag.TextSelectableByKeyboard);cell.addWidget(value);cell.addStretch();self.private_fields[key]=value;private_grid.addWidget(box,0,column);private_grid.setColumnStretch(column,1)
        body.addLayout(private_grid)
        body.addWidget(self.note('Эти данные хранятся только локально. При добавлении события они попадут в выбранный календарь.','lock'))
        self.separator(body);body.addWidget(self.section_heading('Поделиться достижением','share'))
        social=QHBoxLayout();social.setSpacing(8);self.social_buttons={}
        for network,caption in [('telegram','Telegram'),('x','X'),('facebook','Facebook'),('instagram','Instagram')]:
            button=QToolButton();button.setFixedSize(36,36);button.setIconSize(QSize(22,22));button.setObjectName('social_'+network);button.setAccessibleName(caption);button.setToolTip(caption);button.clicked.connect(lambda checked=False,n=network:self.prepare_post(n));self.social_buttons[network]=button;social.addWidget(button)
        divider=QFrame();divider.setObjectName('socialDivider');divider.setFixedSize(1,26);social.addWidget(divider);social.addSpacing(4);social.addWidget(self.button('Сохранить PNG','download',self.export_png));social.addWidget(self.button('Копировать PNG','copy',self.copy_png));social.addStretch();body.addLayout(social)
        body.addWidget(self.note('Только статистика поиска — без даты, времени, офиса и номера подтверждения.','shield'));self.separator(body)
        donation=QHBoxLayout();donation.setSpacing(12);donation.addWidget(FoodSticker());words=QVBoxLayout();words.setSpacing(4);words.addWidget(self.section_heading('Поддержать автора',None));words.addWidget(self.text('Код работает. Автор — за еду.',muted=True));donation.addLayout(words,1)
        card_button=self.payment_button('Картой','GitHub Sponsors','credit_card','https://github.com/sponsors/itwormz');card_button.setToolTip('GitHub Sponsors');donation.addWidget(card_button)
        crypto_button=self.payment_button('Криптовалютой','NOWPayments','bitcoin','https://nowpayments.io/donation/itworm');crypto_button.setToolTip('NOWPayments');donation.addWidget(crypto_button);body.addLayout(donation)
        body.addStretch();self.stack.addWidget(self.achievement_page)
        self.empty_page=QWidget();empty=QVBoxLayout(self.empty_page);empty.setContentsMargins(0,24,0,24);empty.addWidget(self.section_heading('Пока нет отмеченных сит','calendar'));empty.addWidget(self.text('После подтверждения записи на сайте — «Сита получена». Здесь появятся карточка достижения и данные для календаря.',muted=True));empty.addWidget(self.button('Сита получена','check',self.confirm_success,primary=True),0,Qt.AlignmentFlag.AlignLeft);empty.addStretch();self.stack.addWidget(self.empty_page)
        self.message=QLabel();self.message.setProperty('muted',True);self.message.setWordWrap(True);layout.addWidget(self.message)
        self.refresh_timer=QTimer(self);self.refresh_timer.setInterval(2000);self.refresh_timer.timeout.connect(self.poll);self.refresh_timer.start()

    def toggle_budget_distribution(self):
        self.host.controller.send({'command': 'appointment_distribution', 'disabled': not self.budget_disabled})

    def show_budget_history(self, *_):
        budget = self.budget_state
        history = list(budget.get('episode_history', ()))
        if budget.get('first_cod_oper'):
            history.append({
                'first': budget['first_cod_oper'], 'last': budget['last_cod_oper'],
                'offices': budget.get('episode_offices', ()), 'recovered': None,
            })
        results = {
            'cod_oper': 'Сит нет с cod oper', 'booking': 'Показан выбор даты',
            'request_no_slots': 'Сит нет', 'request_no_slots_restart': 'Сит нет',
            'post_contact_no_slots': 'Сит нет',
        }
        lines = []
        for episode in reversed(history):
            first, last = (episode[key].replace('T', ' ')[:16] for key in ('first', 'last'))
            lines.append(f'Первый cod oper: {first} · Последний: {last}')
            if episode['recovered']:
                recovered = episode['recovered'].replace('T', ' ')[:16]
                lines.append(f'Восстановление между {last} и {recovered}')
            for office in episode['offices']:
                result = results.get(office['result'], 'Исход неизвестен')
                lines.append(f"{office['at'][11:16]} · {', '.join(office['names'])} · {result}")
            lines.append('')
        self.history_panel.select_tab(0)
        detail = self.history_panel.restriction_detail
        detail.setText(self.tr_text('\n'.join(lines) if lines else 'Эпизодов пока нет'))
        detail.show()
        from PySide6.QtWidgets import QScrollArea
        parent = self.parentWidget()
        while parent is not None and not isinstance(parent, QScrollArea):
            parent = parent.parentWidget()
        if parent is not None:
            parent.ensureWidgetVisible(detail, 0, 12)

    def refresh_appointment_budget(self):
        from datetime import datetime
        from ..appointment_budget_store import BudgetStore
        from ..appointment_schedule import MADRID

        profile_id = self.profile_combo.currentData()
        controller = self.host.controller
        if controller.active and profile_id == getattr(controller, 'profile_id', None):
            current = self.host.state.get('appointment_budget')
            if current:
                self.show_appointment_budget(current, live=True)
                return
        profile = next((p for p in self.profiles if p.id == profile_id), None)
        budget = None
        if profile is not None:
            try:
                root = self.host.store.path.parent / 'data' / 'appointment-budget'
                stored = BudgetStore.load_profile(root, profile.id)
                if stored is not None:
                    budget = stored.summary(datetime.now(MADRID), profile.appointment_protection)
            except (OSError, ValueError, TypeError, KeyError):
                pass
        self.show_appointment_budget(budget, live=False)

    def show_appointment_budget(self, budget, *, live=True):
        self.budget_state = budget or {}
        self.budget_icon.setVisible(bool(budget))
        self.budget_label.setVisible(bool(budget))
        self.budget_menu.setVisible(bool(budget))
        self.budget_action.setEnabled(live)
        for row in self.budget_availability_rows:
            row.setVisible(bool(budget))
        if not budget:
            return
        self.budget_disabled = budget['disabled']
        self.budget_action.setText(self.tr_text('Вернуть распределение' if self.budget_disabled else 'Отключить распределение на сегодня'))
        self.budget_history_action.setText(self.tr_text('История возможных ограничений'))
        ready = budget['ready'][11:16]
        text = f"<b>Защита от бана</b> · Ориентир {budget['target']} · Остаток ≈{budget['remaining']}"
        if self.budget_disabled:
            text += ' · Распределение отключено'
        if budget['restricted']:
            text += f' · <a href="restriction">⚠ Возможное ограничение · Проба с {ready}</a>'
        for key, value in self.budget_availability.items():
            value.setText(str(budget[key]))
        self.budget_label.setText(self.tr_text(text))

    def text(self,source,muted=False):
        label=QLabel(source);label.setWordWrap(True);label.setTextFormat(Qt.TextFormat.PlainText);label.setProperty('muted',muted);self._text_bindings.append((label,source));return label

    def icon_label(self,name):
        label=QLabel();label.setFixedSize(18,18);self._icon_bindings.append((label,name));return label

    def button(self,source,icon,callback,primary=False):
        button=QPushButton(source);button.setProperty('primary',primary);button.clicked.connect(callback);button.setIconSize(QSize(17,17));self._text_bindings.append((button,source));self._icon_bindings.append((button,icon));return button

    def section_heading(self,title,icon):
        frame=QWidget();row=QHBoxLayout(frame);row.setContentsMargins(0,0,0,0);row.setSpacing(8)
        if icon:row.addWidget(self.icon_label(icon))
        label=self.text(title);label.setObjectName('statisticsSection');row.addWidget(label,1);return frame

    def note(self,title,icon):
        frame=QWidget();row=QHBoxLayout(frame);row.setContentsMargins(0,0,0,0);row.setSpacing(7);row.addWidget(self.icon_label(icon));row.addWidget(self.text(title,muted=True),1);return frame

    def payment_button(self,title,service,icon,url):
        button=QPushButton();button.setObjectName('donationPayment');button.setCursor(Qt.CursorShape.PointingHandCursor);button.setAccessibleName(service);button.clicked.connect(lambda:self.open_url(url))
        row=QHBoxLayout(button);row.setContentsMargins(12,8,12,8);row.setSpacing(9);image=self.icon_label(icon);image.setFixedSize(23,23);row.addWidget(image)
        words=QVBoxLayout();words.setSpacing(2);label=self.text(title);words.addWidget(label);words.addWidget(self.text(service,muted=True));row.addLayout(words)
        for child in button.findChildren(QWidget):child.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self._payment_buttons.append(button);return button

    def statistics_overview(self,layout):
        self.budget_availability = {}
        self.budget_availability_rows = []
        values={};overview=QHBoxLayout();overview.setSpacing(16)
        for key,title,icon in [('checks','Проверок сит','search'),('active_seconds','Активный поиск','clock'),('days','Дни с поиском','calendar_days')]:
            frame=QFrame();frame.setObjectName('statisticsOverview');cell=QVBoxLayout(frame);cell.setContentsMargins(16,14,16,14);cell.setSpacing(8);number=QLabel('—');number.setObjectName('overviewValue');cell.addWidget(number);cell.addWidget(self.section_heading(title,icon));overview.addWidget(frame,1);values[key]=number
        layout.addLayout(overview)
        effort=QHBoxLayout();effort.setSpacing(32)
        for key,title,icon in [('actions_performed','Действий выполнено','pointer'),('pages_processed','Страниц обработано','file_text')]:
            frame=QFrame();frame.setObjectName('statisticRow');row=QHBoxLayout(frame);row.setContentsMargins(0,8,0,8);row.setSpacing(8)
            row.addWidget(self.icon_label(icon));row.addWidget(self.text(title),1);number=QLabel('—');number.setObjectName('statisticValue');row.addWidget(number);values[key]=number;effort.addWidget(frame,1)
        groups=QHBoxLayout();groups.setSpacing(32)
        for title,icon,keys in [('Доступность','eye',['offices_seen','unique_offices','booking_screens','successes']),('Ошибки и восстановление','wrench',['server_errors','rate_limited','blocked','flow_passes'])]:
            frame=QWidget();column=QVBoxLayout(frame);column.setContentsMargins(0,0,0,0);column.setSpacing(8);column.addWidget(self.section_heading(title,icon))
            for key in keys:
                _,caption,glyph=next((m for m in METRICS if m[0]==key),('successes','Полученные ситы','calendar'))
                row_frame=QFrame();row_frame.setObjectName('statisticRow');row=QHBoxLayout(row_frame);row.setContentsMargins(0,8,0,8);row.setSpacing(8);row.addWidget(self.icon_label(glyph));row.addWidget(self.text({'booking_screens':'Появления выбора даты','successes':'Получено сит','rate_limited':'Ограничений HTTP 429'}.get(key,caption)),1);number=QLabel('—');number.setObjectName('statisticValue');row.addWidget(number);values[key]=number;column.addWidget(row_frame)
            if title == 'Доступность':
                for key, caption in (('banners', 'Сит нет до отправки'), ('no_slots', 'Сит нет после отправки')):
                    metric = QFrame()
                    metric.setObjectName('statisticRow')
                    line = QHBoxLayout(metric)
                    line.setContentsMargins(0, 8, 0, 8)
                    line.addWidget(self.text(caption), 1)
                    value = QLabel('—')
                    value.setObjectName('statisticValue')
                    line.addWidget(value)
                    column.addWidget(metric)
                    metric.hide()
                    self.budget_availability[key] = value
                    self.budget_availability_rows.append(metric)
            groups.addWidget(frame,1)
        layout.addLayout(groups)
        layout.addWidget(self.section_heading('Автоматизация','settings'))
        layout.addLayout(effort);return values

    def metric_grid(self,layout,include_first):
        grid=QGridLayout();grid.setHorizontalSpacing(24);grid.setVerticalSpacing(8);grid.setColumnStretch(0,1);grid.setColumnStretch(1,1);values={}
        for index,(key,title,icon) in enumerate(METRICS):
            if not include_first and key=='first_available_seconds':continue
            frame=QFrame();frame.setObjectName('statisticRow');row=QHBoxLayout(frame);row.setContentsMargins(0,9,0,9);row.setSpacing(8);row.addWidget(self.icon_label(icon));label=self.text(title);label.setWordWrap(False);row.addWidget(label,1)
            number=QLabel('—');number.setObjectName('statisticValue');number.setAlignment(Qt.AlignmentFlag.AlignRight|Qt.AlignmentFlag.AlignVCenter);row.addWidget(number);values[key]=number;grid.addWidget(frame,index//2,index%2)
        layout.addLayout(grid);return values

    def separator(self,layout):
        frame=QFrame();frame.setObjectName('statisticsSeparator');frame.setFixedHeight(1);layout.addWidget(frame)

    def set_profiles(self,profiles,selected=None):
        previous=self.profile_combo.currentData() or selected;self.profiles=profiles;self.profile_combo.blockSignals(True);self.profile_combo.clear()
        for profile in profiles:self.profile_combo.addItem(profile.label,profile.id)
        if previous:self.profile_combo.setCurrentIndex(max(0,self.profile_combo.findData(previous)))
        self.profile_combo.blockSignals(False);self.profile_selected()

    def profile_selected(self,*_):
        was_achievement=self.stack.currentWidget()!=self.stats_page
        self.current_achievement=None;self._snapshot=None;self._render_key=None;self.stack.setCurrentWidget(self.stats_page);self.message.clear()
        self.history_panel.refresh(self.profile_combo.currentData(),self.period,False,loaded={'bounds':None,'office_rows':[],'rows':[],'raw_rows':[]},signature=None)
        self.history_panel.restriction_detail.hide()
        self.refresh_appointment_budget()
        self.refresh()
        if was_achievement:self.show_achievement()

    def set_period(self,period):
        self.period=period;self.refresh()

    def poll(self):
        if self.isVisible() and not self._waiting:self._submit(False)

    def tr_text(self,text):return self.host.t(text)

    def report_read_error(self,component,exc):
        failure=(component,type(exc).__name__)
        if failure!=self._statistics_read_failure:
            log_event(log,logging.WARNING,'profile_statistics_read_failed',component=component,error_type=type(exc).__name__)
        self._statistics_read_failure=failure
        self._statistics_read_message=self.tr_text('Не удалось прочитать статистику профиля')
        self.message.setText(self._statistics_read_message)

    def refresh(self):
        self._render_key=None;self._submit(True)

    def present(self):
        """Translate the statistics view from the last snapshot, without reading storage."""
        language=self.host.preferences.get('language');dark=self.host.preferences.get('dark')
        key=self._view_key(language,dark)
        if key!=self._render_key:
            self._render_key=key;self._render(self._snapshot)
        self._retitle_history(language,dark)

    def _view_key(self,language,dark):
        warning=bool(getattr(self.host,'state',{}).get('statistics_error'))
        achievement=self.current_achievement or {}
        appointment=achievement.get('appointment') or {}
        return (language,dark,warning,self.period,self.profile_combo.currentData(),self.stack.currentWidget()!=self.stats_page,
                id(self._snapshot),achievement.get('id'),tuple(sorted(appointment.items())))

    def _retitle_history(self,language,dark):
        visual=(language,dark)
        if visual==self._history_visual or not self.history_panel.profile_id:return
        self._history_visual=visual
        panel=self.history_panel
        panel.refresh(panel.profile_id,panel.period,panel.baseline,loaded={
            'bounds':panel.bounds,'office_rows':panel.office_rows,'rows':panel.rows,'raw_rows':panel.raw_rows,
        },signature=panel._data_signature)

    def _stale(self,profile_id):
        scope=(profile_id,self.period,datetime.now(MADRID).date() if self.period=='today' else None)
        revision=(scope,history_revision(self.statistics),_file_revision(self.achievements.path))
        return revision!=getattr(self,'_applied_revision',None)

    def _submit(self,wait):
        self.present()
        profile_id=self.profile_combo.currentData()
        if not profile_id:
            self._snapshot=None;self._render_key=None;self.present();return
        if not wait:
            if self._inflight_token:
                self._again=True;return
            if not self._stale(profile_id):return
        elif self._inflight_token:
            self._again=False
        self._generation+=1;token=self._generation;self._inflight_token=token
        scope,custom=self.history_panel.read_plan(profile_id,self.period)
        statistics,achievements,period=self.statistics,self.achievements,self.period
        def work():
            try:result=load_statistics_view(statistics,achievements,profile_id,period,custom,scope)
            except Exception as exc:result=('err','totals',exc)
            self.loaded.emit(token,result)
        if not wait:
            self._jobs.start(_Load(work));return
        # Connect before starting. A fast read can finish before exec(), and the
        # queued callback must already include the waiter or the loop never returns.
        self._waiting=True;loop=QEventLoop(self)
        def stop(got,_result):
            if got==token or token!=self._generation:loop.quit()
        self.loaded.connect(stop)
        try:
            self._jobs.start(_Load(work));loop.exec()
        finally:
            self._waiting=False;self.loaded.disconnect(stop)

    def _on_loaded(self,token,result):
        current=token==self._generation
        if current:self._consume(result)
        if self._inflight_token==token:self._inflight_token=None
        if current and self._again and not self._waiting:
            self._again=False;self._submit(False)

    def _consume(self,result):
        if result[0]=='err':
            self.report_read_error(result[1],result[2]);return
        _,data,records,loaded,signature,profile_id,period,custom=result
        if profile_id!=self.profile_combo.currentData() or period!=self.period:return
        if custom!=self.history_panel.custom_range:
            self._applied_revision=None;return
        self._snapshot=(data,records);self._applied_revision=signature;self._render_key=None
        self.history_panel.refresh(profile_id,period,bool(data.get('legacy_baseline')),loaded=loaded,signature=(signature[0],signature[1]))
        self._history_visual=(self.host.preferences.get('language'),self.host.preferences.get('dark'))
        self._render(self._snapshot);self._render_key=self._view_key(*self._history_visual)

    def _render(self,snapshot):
        t=self.tr_text
        for widget,source in self._text_bindings:widget.setText(t(source))
        self._icon_bindings[0]=(self.heading_icon,'award' if self.stack.currentWidget()!=self.stats_page else 'statistics')
        self.title.setText(t('Достижения профиля' if self.stack.currentWidget()!=self.stats_page else 'Статистика профиля'))
        self.back_button.setVisible(self.stack.currentWidget()!=self.stats_page)
        self.profile_combo.setAccessibleName(t('Профиль для статистики'))
        self.edit_action.setText(t('Исправить данные записи'));self.more.setVisible(self.stack.currentWidget()==self.achievement_page);self.edit_action.setEnabled(bool(self.current_achievement))
        if hasattr(self.host,'control_icon') and hasattr(self.host,'preferences'):
            for widget,name in self._icon_bindings:
                if not name:continue
                icon=self.host.control_icon(name)
                if isinstance(widget,QLabel):widget.setPixmap(icon.pixmap(18,18))
                else:widget.setIcon(icon)
        for button in self._payment_buttons:
            button.layout().invalidate();button.setFixedSize(button.layout().sizeHint())
        for widget,name in self._icon_bindings:
            if name=='bitcoin' and isinstance(widget,QLabel):
                pixmap=QPixmap(46,46);pixmap.setDevicePixelRatio(2);pixmap.fill(Qt.GlobalColor.transparent);painter=QPainter(pixmap);painter.setRenderHint(QPainter.RenderHint.Antialiasing);colors=theme_colors(self.host.preferences['dark']);painter.setPen(Qt.PenStyle.NoPen);painter.setBrush(QColor(colors['bitcoin']));painter.drawEllipse(0,0,23,23);painter.setPen(QColor(colors['on_brand']));font=QFont('IBM Plex Sans',15);font.setBold(True);painter.setFont(font);painter.drawText(0,0,23,23,Qt.AlignmentFlag.AlignCenter,'₿');painter.end();widget.setPixmap(pixmap)
        for network,button in self.social_buttons.items():
            asset=Path(__file__).parent/'assets'/'social'/f'{network}.svg'
            if asset.exists():button.setIcon(QIcon(str(asset)))
            else:button.setText({'telegram':'↗','x':'𝕏','facebook':'f','instagram':'◎'}[network])
        self.today_button.setChecked(self.period=='today');self.all_button.setChecked(self.period=='all')
        warning=bool(getattr(self.host,'state',{}).get('statistics_error'));self.statistics_warning.setVisible(warning)
        profile_id=self.profile_combo.currentData();self.success_button.setEnabled(bool(profile_id))
        if not profile_id:self.achievement_button.hide();self.first_value.setText(t('Ещё не появлялись'));return
        if not snapshot:
            for widget in self.stat_values.values():widget.setText('—')
            self.first_value.setText('—');self.first_booking_value.setText('—');self.achievement_button.hide();return
        data,records=snapshot
        if self._statistics_read_failure:
            if self.message.text()==self._statistics_read_message:self.message.clear()
            self._statistics_read_failure=None;self._statistics_read_message=None
        for key,widget in self.stat_values.items():
            if key=='days':widget.setText('—' if not data.get('days') and any(data.get(k) for k in ('checks','active_seconds','offices_seen','booking_screens')) else str(data.get('days',0)))
            elif key in {'actions_performed','pages_processed'}:
                widget.setText(str(data[key]) if data.get('effort_started_at') else '—')
            elif key=='successes':
                visible_records=records if self.period=='all' else [record for record in records if datetime.fromisoformat(record['recorded_at'].replace('Z','+00:00')).astimezone(MADRID).date()==datetime.now(MADRID).date()]
                widget.setText(str(len(visible_records)))
            else:widget.setText(duration(data.get(key),t) if key.endswith('seconds') else str(data.get(key,0)))
        for prefix,widget,caption in [('first_office',self.first_value,self.first_caption),('first_booking',self.first_booking_value,self.first_booking_caption)]:
            when=data.get(prefix+'_at');seconds=data.get(prefix+'_seconds');quality=data.get(prefix+'_active_quality','unknown');status=data.get(prefix+'_status','unknown')
            base='Первый офис' if prefix=='first_office' else 'Первый выбор даты'
            caption.setText(t(base+' сегодня' if self.period=='today' else base+' в истории' if status=='recorded' else base))
            text=stamp(when) if when else t('Дата неизвестна') if status=='unknown' else t('Ещё не появлялись')
            if when and seconds is not None:
                amount=f'{int(seconds)} {t("с")}' if seconds<60 else duration(seconds,t)
                text+=' · '+('≥ ' if quality=='minimum' else '')+t('{duration} поиска').format(duration=amount)
            widget.setText(text)
            hints=['Europe/Madrid']
            if status=='recorded':hints.append(t('Первое событие в сохранённой истории; более ранние события могли не сохраниться.'))
            if quality=='minimum':hints.append(t('Не менее указанного времени: учтены подтверждённые интервалы, в истории есть пропуски.'))
            elif when and seconds is None:hints.append(t('Дата известна; длительность активного поиска до события не сохранена.'))
            if data.get('legacy_first_available_at'):
                hints.append(t('Старая первая доступность')+': '+stamp(data['legacy_first_available_at']))
            widget.setToolTip('\n'.join(hints));caption.setToolTip(widget.toolTip())
        for key,widget in self.stat_values.items():
            widget.setToolTip(t('До обновления этот показатель не сохранялся; показаны новые наблюдения.') if key in data.get('legacy_unknown_fields',[]) else '')
            if key in {'actions_performed','pages_processed'}:
                started=data.get('effort_started_at')
                widget.setToolTip(t('Учёт с {time}. Ранние действия и страницы не восстанавливаются.').format(time=stamp(started)) if started else t('Ранее этот показатель не сохранялся.'))
        self.achievement_button.setVisible(bool(records));self.achievement_button.setEnabled(bool(records))
        if self.current_achievement:
            stats=self.current_achievement['statistics']
            for key,widget in self.achievement_values.items():widget.setText(duration(stats.get(key),t) if key.endswith('seconds') else ('—' if stats.get(key) is None else str(stats.get(key,0))))
            self.achievement_duration.setText(public_duration(self.current_achievement, self.host.preferences["language"]))
            a=self.current_achievement['appointment'];self.private_fields['date'].setText(a.get('date','')+' · '+a.get('time',''));self.private_fields['office'].setText('\n'.join(filter(None,[a.get('office',''),a.get('address','')])) or '—');self.private_fields['confirmation'].setText(a.get('confirmation','') or '—')

    def fit_current_page(self,*_):
        for index in range(self.stack.count()):
            policy=QSizePolicy.Policy.Preferred if index==self.stack.currentIndex() else QSizePolicy.Policy.Ignored
            self.stack.widget(index).setSizePolicy(policy,policy)
        self.stack.layout().invalidate();self.stack.updateGeometry();self.layout().invalidate();self.updateGeometry()

    def show_statistics(self):
        self.current_achievement=None;self.stack.setCurrentWidget(self.stats_page);self.message.clear();self.refresh()

    def show_achievement(self):
        try:records=self.achievements.list(self.profile_combo.currentData())
        except (OSError,ValueError) as exc:self.report_read_error('achievements',exc);return
        if records:self.current_achievement=records[-1];self.stack.setCurrentWidget(self.achievement_page)
        else:self.current_achievement=None;self.stack.setCurrentWidget(self.empty_page)
        self.refresh()

    def confirm_success(self):
        return self.appointment_form(False)

    def edit_appointment(self):
        if self.current_achievement:return self.appointment_form(True)

    def appointment_form(self,editing=False):
        profile_id=self.profile_combo.currentData()
        if not profile_id:return
        from .appointment_form import AppointmentForm
        task=next((p for p in self.profiles if p.id==profile_id),None);province=getattr(task,'province','')
        known=self.statistics.known_offices(profile_id)
        labels=[label[len(province)+2:] if label.startswith(province+': ') else label for label in known if not province or label.startswith(province+': ')]
        selected=getattr(self.host,'state',{}).get('selected_office','') if getattr(self.host.controller,'profile_id',None)==profile_id else ''
        if not selected:
            last=self.statistics.last_office(profile_id)
            if last and (not province or last.startswith(province+': ')):
                selected=last[len(province)+2:] if province else last
        saved=self.current_achievement['appointment'] if editing else None
        dialog=AppointmentForm(self.host,labels,selected=selected,saved=saved)
        if dialog.exec()!=QDialog.DialogCode.Accepted:return
        zone='Atlantic/Canary' if 'PALMAS' in province.upper() or 'TENERIFE' in province.upper() else 'Europe/Madrid'
        appointment=dialog.details(zone);t=self.tr_text
        try:
            if editing:self.current_achievement=self.achievements.update_appointment(profile_id,self.current_achievement['id'],appointment)
            else:self.current_achievement=self.achievements.record(profile_id,self.statistics.get(profile_id),appointment)
        except (OSError,ValueError):self.message.setText(t('Не удалось сохранить достижение'));return
        active_id=getattr(self.host.controller,'profile_id',None)
        if not editing and self.host.controller.active and active_id==profile_id:self.host.controller.send({'command':'stop'})
        self.stack.setCurrentWidget(self.achievement_page);self.refresh()

    def open_url(self,url):QDesktopServices.openUrl(QUrl(url))

    def image(self):
        prefs=self.host.preferences
        return render_card(self.current_achievement,language=prefs['language'],dark=prefs['dark'])

    def export_png(self):
        if not self.current_achievement:return
        path,_=QFileDialog.getSaveFileName(self,self.tr_text('Сохранить PNG'),'cita-achievement.png','PNG (*.png)')
        if not path:return
        try:
            if not self.image().save(path,'PNG'):raise OSError('PNG export failed')
            self.message.setText(self.tr_text('Картинка сохранена'))
        except (OSError,ValueError):self.message.setText(self.tr_text('Не удалось сохранить картинку'))

    def copy_png(self):
        if self.current_achievement:
            try:QApplication.clipboard().setImage(self.image());self.message.setText(self.tr_text('Картинка скопирована'))
            except (OSError,ValueError):self.message.setText(self.tr_text('Не удалось сохранить картинку'))

    def export_calendar(self):
        if not self.current_achievement:return
        from .calendar_dialog import CalendarDialog
        CalendarDialog(self.host,self.current_achievement).exec()

    def prepare_post(self,network):
        if not self.current_achievement:return
        from .share_dialog import ShareDialog
        dialog=ShareDialog(self.host,self.current_achievement,network);dialog.exec()
