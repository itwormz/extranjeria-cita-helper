"""Per-profile observations and locally confirmed appointment achievements."""
from datetime import datetime, timezone
from pathlib import Path

from PySide6.QtCore import Qt, QDate, QTime, QTimer, QUrl, QSize, Signal
from PySide6.QtGui import QDesktopServices, QPainter, QColor, QFont, QIcon, QPixmap
from PySide6.QtWidgets import (QWidget, QLabel, QPushButton, QToolButton, QComboBox,
    QVBoxLayout, QHBoxLayout, QGridLayout, QStackedWidget, QFrame, QDialog,
    QDialogButtonBox, QSizePolicy, QDateEdit, QTimeEdit, QAbstractSpinBox, QLineEdit, QFileDialog, QApplication, QMenu, QLayout)
from .theme import theme_colors
from .statistics import ProfileStatistics
from .achievements import AchievementStore
from .achievement_exports import public_post, write_calendar, render_card, share_url, public_duration

METRICS=(('checks','Проверок','search'),('active_seconds','Активный поиск','clock'),
    ('unique_offices','Разных офисов','pin'),('booking_screens','Экранов ситы','calendar'),
    ('offices_seen','Появлений офисов','office'),('server_errors','Ошибок сервера','server'),
    ('rate_limited','HTTP 429','clock'),('blocked','Блокировок','shield'),
    ('flow_passes','Проходов с начала','restart'),('first_available_seconds','До первой доступности','clock'))


def duration(seconds,t):
    if seconds is None:return '—'
    minutes=int(seconds)//60
    hours,minutes=divmod(minutes,60)
    return f'{hours} {t("ч")} {minutes} {t("мин")}' if hours else f'{minutes} {t("мин")}'


class CurrentPageStack(QWidget):
    """Only the visible page participates in layout sizing."""
    currentChanged=Signal(int)

    def __init__(self):
        super().__init__();self.pages=[];self.current=-1
        layout=QVBoxLayout(self);layout.setContentsMargins(0,0,0,0);layout.setSpacing(0)

    def addWidget(self,page):
        self.pages.append(page);self.layout().addWidget(page);page.hide()
        if self.current<0:self.setCurrentWidget(page)

    def currentWidget(self):
        return self.pages[self.current] if self.current>=0 else None

    def currentIndex(self):return self.current
    def count(self):return len(self.pages)
    def widget(self,index):return self.pages[index]

    def setCurrentWidget(self,page):
        index=self.pages.index(page)
        if index==self.current:return
        if self.current>=0:self.pages[self.current].hide()
        self.current=index;page.show();self.layout().invalidate();self.updateGeometry();self.currentChanged.emit(index)


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


class StatisticsPanel(QWidget):
    def __init__(self,host):
        super().__init__();self.setObjectName("statisticsPanel");self.host=host;self.profiles=[];self.current_achievement=None;self._signature=None
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
        self.stat_values=self.statistics_overview(self.stats_layout)
        first_separator=QFrame();first_separator.setObjectName('journalSeparator');first_separator.setFixedHeight(1);self.stats_layout.addWidget(first_separator)
        first_row=QHBoxLayout();first_row.setSpacing(12);first_row.addWidget(self.icon_label('clock'))
        self.first_caption=self.text('До первого офиса или ситы',muted=True);self.first_caption.setWordWrap(False);first_row.addWidget(self.first_caption)
        self.first_value=QLabel('—');first_row.addWidget(self.first_value);first_row.addStretch();self.stats_layout.addLayout(first_row)
        self.success_note=self.text('После оформления ситы можно сохранить данные записи, добавить её в календарь и поделиться статистикой поиска.',muted=True)
        self.stats_layout.addWidget(self.success_note)
        actions=QHBoxLayout();actions.setSpacing(12);self.success_button=self.button('Отметить получение ситы','calendar',self.confirm_success,primary=True);actions.addWidget(self.success_button)
        self.achievement_button=self.button('Достижения профиля','calendar',self.show_achievement);actions.addWidget(self.achievement_button);actions.addStretch();self.stats_layout.addLayout(actions)
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
        values={};overview=QHBoxLayout();overview.setSpacing(16)
        for key,title,icon in [('checks','Проверок сит','search'),('active_seconds','Активный поиск','clock'),('days','С начала поиска','calendar_days')]:
            frame=QFrame();frame.setObjectName('statisticsOverview');cell=QVBoxLayout(frame);cell.setContentsMargins(16,14,16,14);cell.setSpacing(8);number=QLabel('—');number.setObjectName('overviewValue');cell.addWidget(number);cell.addWidget(self.section_heading(title,icon));overview.addWidget(frame,1);values[key]=number
        layout.addLayout(overview);groups=QHBoxLayout();groups.setSpacing(32)
        for title,icon,keys in [('Доступность','eye',['offices_seen','unique_offices','booking_screens','successes']),('Ошибки и восстановление','wrench',['server_errors','rate_limited','blocked','flow_passes'])]:
            frame=QWidget();column=QVBoxLayout(frame);column.setContentsMargins(0,0,0,0);column.setSpacing(8);column.addWidget(self.section_heading(title,icon))
            for key in keys:
                _,caption,glyph=next((m for m in METRICS if m[0]==key),('successes','Полученные ситы','calendar'))
                row_frame=QFrame();row_frame.setObjectName('statisticRow');row=QHBoxLayout(row_frame);row.setContentsMargins(0,8,0,8);row.setSpacing(8);row.addWidget(self.icon_label(glyph));row.addWidget(self.text({'booking_screens':'Экранов выбора ситы','successes':'Получено сит','rate_limited':'Ограничений HTTP 429'}.get(key,caption)),1);number=QLabel('—');number.setObjectName('statisticValue');row.addWidget(number);values[key]=number;column.addWidget(row_frame)
            groups.addWidget(frame,1)
        layout.addLayout(groups);return values

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
        self.current_achievement=None;self.stack.setCurrentWidget(self.stats_page);self.message.clear();self.refresh()
        if was_achievement:self.show_achievement()

    def poll(self):
        if self.isVisible():self.refresh()

    def tr_text(self,text):return self.host.t(text)

    def refresh(self):
        t=self.tr_text
        for widget,source in self._text_bindings:widget.setText(t(source))
        self._icon_bindings[0]=(self.heading_icon,'award' if self.stack.currentWidget()!=self.stats_page else 'statistics')
        self.title.setText(t('Достижения профиля' if self.stack.currentWidget()!=self.stats_page else 'Статистика профиля'))
        self.back_button.setVisible(self.stack.currentWidget()!=self.stats_page)
        self.profile_combo.setAccessibleName(t('Профиль для статистики'))
        self.edit_action.setText(t('Исправить данные записи'));self.more.setVisible(self.stack.currentWidget()==self.achievement_page);self.edit_action.setEnabled(bool(self.current_achievement))
        if hasattr(self.host,'control_icon') and hasattr(self.host,'preferences'):
            for widget,name in self._icon_bindings:
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
        profile_id=self.profile_combo.currentData();self.success_button.setEnabled(bool(profile_id))
        if not profile_id:self.achievement_button.hide();self.first_value.setText(t('Ещё не появлялись'));return
        try:data=self.statistics.get(profile_id);records=self.achievements.list(profile_id)
        except (OSError,ValueError) as exc:self.message.setText(t('Не удалось прочитать статистику профиля'));return
        for key,widget in self.stat_values.items():
            if key=='days':
                started=data.get('started_at');days=max(0,(datetime.now(timezone.utc)-datetime.fromisoformat(started)).days) if started else None;widget.setText(t('Менее суток') if days==0 else str(days)+' '+t('дн.') if days is not None else '—')
            elif key=='successes':widget.setText(str(len(records)))
            else:widget.setText(duration(data.get(key),t) if key.endswith('seconds') else str(data.get(key,0)))
        seconds=data.get('first_available_seconds')
        if seconds is None:self.first_value.setText(t('Ещё не появлялись'))
        else:
            days,hours=divmod(int(seconds)//3600,24)
            self.first_value.setText(f'{days} {t("дн.")} {hours} {t("ч")}' if days else duration(seconds,t))
        self.first_caption.setToolTip(t('Время от начала поиска до первого офиса или экрана выбора ситы'))
        self.achievement_button.setVisible(bool(records));self.achievement_button.setEnabled(bool(records))
        if self.current_achievement:
            stats=self.current_achievement['statistics']
            for key,widget in self.achievement_values.items():widget.setText(duration(stats.get(key),t) if key.endswith('seconds') else str(stats.get(key,0)))
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
        except (OSError,ValueError):self.message.setText(self.tr_text('Не удалось прочитать статистику профиля'));return
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
