"""Explicit Google Calendar or local ICS preparation without booking secrets."""
from pathlib import Path
from PySide6.QtCore import Qt,QUrl,QDate,QLocale,QSize
from PySide6.QtGui import QDesktopServices,QIcon
from PySide6.QtWidgets import QDialog,QVBoxLayout,QHBoxLayout,QLabel,QPushButton,QFileDialog,QFrame
from .achievement_exports import write_calendar,google_calendar_url,CALENDAR_TITLE


class CalendarDialog(QDialog):
    def __init__(self,host,achievement):
        super().__init__(host);self.host=host;self.achievement=achievement;self.saved_path=None;t=host.t
        self.setWindowTitle(t('Добавить в календарь'));self.setMinimumWidth(500);layout=QVBoxLayout(self);layout.setContentsMargins(22,20,22,20);layout.setSpacing(14)
        event=QFrame();event.setObjectName('calendarPreview');box=QVBoxLayout(event);box.setContentsMargins(14,12,14,12);box.setSpacing(7);title=QLabel(CALENDAR_TITLE);title.setObjectName('statisticsSection');box.addWidget(title)
        appointment=achievement['appointment'];locale=QLocale({'ru':'ru_RU','es':'es_ES'}.get(host.preferences['language'],'en_GB'));date=QDate.fromString(appointment.get('date',''),'yyyy-MM-dd');value=locale.toString(date,'d MMMM yyyy')+' · '+appointment.get('time','');box.addWidget(self.label(value));note=self.label(t('Местное время офиса'));note.setProperty('muted',True);box.addWidget(note)
        box.addWidget(self.label(appointment.get('office','')));address=self.label(appointment.get('address',''));address.setProperty('muted',True);box.addWidget(address);layout.addWidget(event)
        choices=QHBoxLayout();self.google=QPushButton('Google Calendar');self.google.setIcon(QIcon(str(Path(__file__).parent/'assets/brands/google-calendar.png')));self.google.setIconSize(QSize(22,22));self.google.clicked.connect(self.open_google);choices.addWidget(self.google)
        self.local=QPushButton(t('Сохранить .ics'));self.local.clicked.connect(self.save_file)
        if hasattr(host,'control_icon'):self.local.setIcon(host.control_icon('download'))
        choices.addWidget(self.local);choices.addStretch();layout.addLayout(choices)
        privacy=self.label(t('Номер подтверждения не включается.'));privacy.setProperty('muted',True);layout.addWidget(privacy)
        self.result=QFrame();self.result.setObjectName('shareResult');result_layout=QVBoxLayout(self.result);result_layout.setContentsMargins(12,10,12,10);result_layout.setSpacing(8);self.result_title=self.label('');self.result_title.setObjectName('calendarResultTitle');result_layout.addWidget(self.result_title);self.result_body=self.label('');self.result_body.setProperty('muted',True);result_layout.addWidget(self.result_body);self.open_button=QPushButton(t('Открыть в календаре'));self.open_button.setProperty('primary',True);self.open_button.clicked.connect(self.open_file);result_layout.addWidget(self.open_button,0,Qt.AlignmentFlag.AlignLeft);layout.addWidget(self.result);self.result.hide()
        row=QHBoxLayout();row.addStretch();close=QPushButton(t('Закрыть'));close.clicked.connect(self.reject);row.addWidget(close);layout.addLayout(row)

    def label(self,text):
        label=QLabel(text);label.setWordWrap(True);label.setTextFormat(Qt.TextFormat.PlainText);return label

    def show_result(self,title,body,open_file=False):
        self.result_title.setText(self.host.t(title));self.result_body.setText(self.host.t(body));self.open_button.setVisible(open_file);self.result.show()

    def open_google(self):
        try:url=google_calendar_url(self.achievement)
        except (ValueError,KeyError):self.show_result('Проверить дату и время записи','Исправить данные записи перед добавлением в календарь.');return
        if QDesktopServices.openUrl(QUrl(url)):
            self.show_result('Форма Google Calendar открыта','Выбрать календарь и напоминание, затем нажать «Сохранить» в Google Calendar.')
        else:self.show_result('Не удалось открыть Google Calendar','Повторить попытку или сохранить файл .ics.')

    def save_file(self):
        path,_=QFileDialog.getSaveFileName(self,self.host.t('Сохранить .ics'),'cita-appointment.ics','iCalendar (*.ics)')
        if not path:return
        try:
            self.saved_path=write_calendar(self.achievement,Path(path))
        except (OSError,ValueError,KeyError):self.show_result('Не удалось сохранить событие','Проверить данные записи и выбранную папку.');return
        self.show_result('Файл события сохранён','Открыть файл в календаре и подтвердить добавление.',open_file=True)

    def open_file(self):
        if self.saved_path and not QDesktopServices.openUrl(QUrl.fromLocalFile(str(self.saved_path.resolve()))):
            self.show_result('Не удалось открыть календарь','Импортировать сохранённый файл .ics в календарное приложение.',open_file=True)
