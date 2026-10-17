"""Compact, local appointment note with explicit validated date/time input."""
import re
from datetime import date as calendar_date
from PySide6.QtCore import Qt, QDate, QPoint, QSize, QLocale, QEvent
from PySide6.QtGui import QColor, QTextCharFormat, QIcon, QTransform
from PySide6.QtWidgets import (QDialog, QFrame, QWidget, QLineEdit, QComboBox,
    QCalendarWidget, QLabel, QPushButton, QToolButton, QVBoxLayout, QHBoxLayout,
    QStyledItemDelegate, QStyleOptionViewItem, QStyle)
from .theme import theme_colors


def format_date(value):
    value=value.strip()
    if re.fullmatch(r'\d{8}',value):value=f'{value[:2]}.{value[2:4]}.{value[4:]}'
    match=re.fullmatch(r'(\d{1,2})[./-](\d{1,2})[./-](\d{4})',value)
    if not match:return None
    day,month,year=map(int,match.groups())
    try:calendar_date(year,month,day)
    except ValueError:return None
    return f'{day:02}.{month:02}.{year:04}'


def format_time(value):
    value=value.strip()
    if re.fullmatch(r'\d{4}',value):value=f'{value[:2]}:{value[2:]}'
    match=re.fullmatch(r'(\d{1,2}):(\d{2})',value)
    if not match:return None
    hour,minute=map(int,match.groups())
    return f'{hour:02}:{minute:02}' if hour<24 and minute<60 else None


def split_office(label):
    name,separator,address=label.partition(',')
    return name.strip(),address.strip() if separator else ''


class SeparatedDateTimeEdit(QLineEdit):
    """Insert separators without moving digits between existing date/time parts."""
    def __init__(self,kind):
        super().__init__();self.kind=kind;self.separator='.' if kind=='date' else ':'
        self.setMaxLength(10 if kind=='date' else 5);self.textEdited.connect(self.format_live)

    def format_live(self,text):
        position=self.cursorPosition();prefix=text[:position]
        cleaned=re.sub(r'[^0-9.:/-]','',text)
        if self.kind=='date':cleaned=cleaned.replace('/','.').replace('-','.')
        else:cleaned=cleaned.replace('.',':').replace('/',':').replace('-',':')
        sep=self.separator
        if sep not in cleaned:
            digits=re.sub(r'[^0-9]','',cleaned)
            groups=[digits[:2],digits[2:4]] if self.kind=='time' else [digits[:2],digits[2:4],digits[4:8]]
            formatted=groups[0]
            if len(groups[0])==2:formatted+=sep+groups[1]
            if self.kind=='date' and len(groups[1])==2:formatted+=sep+groups[2]
        else:
            parts=cleaned.split(sep);limits=[2,2] if self.kind=='time' else [2,2,4]
            parts=[re.sub(r'[^0-9]','',part)[:limit] for part,limit in zip(parts,limits)]
            while len(parts)>1 and not parts[-1] and len(parts[-2])<2:parts.pop()
            if len(parts)<len(limits) and len(parts[-1])==2:parts.append('')
            formatted=sep.join(parts)
        digits_before=sum(c.isdigit() for c in prefix);cursor=0;seen=0
        for i,char in enumerate(formatted):
            if char.isdigit():seen+=1
            if seen<=digits_before:cursor=i+1
            if seen==digits_before and char.isdigit():
                cursor=i+1
                if cursor<len(formatted) and formatted[cursor]==sep and len(formatted[:cursor].rsplit(sep,1)[-1])==2:cursor+=1
                break
        if digits_before==0:cursor=0
        if formatted!=text:self.setText(formatted)
        self.setCursorPosition(cursor)

    def keyPressEvent(self,event):
        if not self.hasSelectedText():
            position=self.cursorPosition();text=self.text()
            if event.key()==Qt.Key.Key_Backspace and position and text[position-1]==self.separator:self.setCursorPosition(position-1)
            elif event.key()==Qt.Key.Key_Delete and position<len(text) and text[position]==self.separator:self.setCursorPosition(position+1)
        super().keyPressEvent(event)


class OfficeDelegate(QStyledItemDelegate):
    def paint(self,painter,option,index):
        styled=QStyleOptionViewItem(option);styled.state &= ~QStyle.StateFlag.State_HasFocus
        super().paint(painter,styled,index)


class AppointmentForm(QDialog):
    def __init__(self,host,offices,selected='',saved=None):
        super().__init__(host);self.host=host;t=host.t;self.setWindowTitle(t('Сита получена'));self.setMinimumWidth(540)
        layout=QVBoxLayout(self);layout.setContentsMargins(24,20,24,20);layout.setSpacing(12)
        title=QLabel(t('Сохранить запись для напоминания в календаре.'));title.setWordWrap(True);title.setProperty('muted',True);layout.addWidget(title)
        self.date=SeparatedDateTimeEdit('date');self.date.setText(QDate.currentDate().toString('dd.MM.yyyy'));self.date.setObjectName('appointmentDate');self.date.setMaxLength(10);self.date.setFixedWidth(98)
        self.time=SeparatedDateTimeEdit('time');self.time.setText('09:00');self.time.setObjectName('appointmentTime');self.time.setMaxLength(5);self.time.setFixedWidth(58)
        for field in (self.date,self.time):field.setAlignment(Qt.AlignmentFlag.AlignLeft);field.setInputMethodHints(Qt.InputMethodHint.ImhDigitsOnly)
        self.calendar_button=QToolButton();self.calendar_button.setIcon(host.control_icon('calendar_days') if hasattr(host,'control_icon') else host.style().standardIcon(QStyle.StandardPixmap.SP_FileDialogDetailedView));self.calendar_button.setAccessibleName(t('Открыть календарь'));self.calendar_button.setFixedSize(24,24);self.calendar_button.setObjectName('appointmentCalendarButton')
        row=QHBoxLayout();row.setSpacing(12);self.date_box,self.date_error=self.input_box(t('Дата'),self.date,inline=True,extra=self.calendar_button);self.time_box,self.time_error=self.input_box(t('Время'),self.time,inline=True);row.addWidget(self.date_box,1);row.addWidget(self.time_box,1);layout.addLayout(row)
        self.office=QComboBox();self.office.setEditable(True);self.office.setInsertPolicy(QComboBox.InsertPolicy.NoInsert);self.office.setObjectName('appointmentOfficeCombo');self.office.lineEdit().setObjectName('appointmentOffice');self.office.lineEdit().setPlaceholderText(t('Выбрать офис или ввести название'))
        self.office.view().setItemDelegate(OfficeDelegate(self.office.view()));self.office.view().setObjectName('appointmentOfficePopup')
        self.address=QLineEdit();self.address.setObjectName('appointmentAddress');self.reference=QLineEdit();self.reference.setObjectName('appointmentConfirmation');self.reference.setPlaceholderText(t('Необязательно'))
        self.parts={}
        for label in offices:
            name,address=split_office(label);self.parts[label]=(name,address);self.office.addItem(label,(name,address))
        self.office.setCurrentIndex(-1);self.office.setCurrentText('');self.office.activated.connect(self.office_selected)
        for caption,field in [('Офис',self.office),('Адрес',self.address),('Номер подтверждения',self.reference)]:
            box,_=self.input_box(t(caption),field)
            if field is self.reference:box.setMaximumWidth(280)
            layout.addWidget(box)
        if saved:
            self.date.setText(QDate.fromString(saved.get('date',''),'yyyy-MM-dd').toString('dd.MM.yyyy'));self.time.setText(saved.get('time',''));self.office.setCurrentText(saved.get('office',''));self.address.setText(saved.get('address',''));self.reference.setText(saved.get('confirmation',''))
        elif selected:
            name,address=split_office(selected);self.office.setCurrentText(name);self.address.setText(address)
        privacy=QLabel(t('Данные остаются на этом компьютере.'));privacy.setProperty('muted',True);layout.addWidget(privacy)
        actions=QHBoxLayout();actions.addStretch();cancel=QPushButton(t('Отмена'));cancel.clicked.connect(self.reject);actions.addWidget(cancel);self.save=QPushButton(t('Сохранить'));self.save.setProperty('primary',True);self.save.setDefault(True);self.save.clicked.connect(self.accept);actions.addWidget(self.save);layout.addLayout(actions)
        self.calendar_popup=QFrame(self,Qt.WindowType.Popup);self.calendar_popup.setObjectName('appointmentCalendarPopup');popup_layout=QVBoxLayout(self.calendar_popup);popup_layout.setContentsMargins(8,8,8,8)
        self.calendar=QCalendarWidget();self.calendar.setObjectName('appointmentCalendar');self.calendar.setGridVisible(False);self.calendar.setVerticalHeaderFormat(QCalendarWidget.VerticalHeaderFormat.NoVerticalHeader);self.calendar.setFirstDayOfWeek(Qt.DayOfWeek.Monday);self.calendar.setLocale(QLocale({'ru':'ru_RU','es':'es_ES'}.get(host.preferences['language'],'en_GB')));popup_layout.addWidget(self.calendar)
        colors=theme_colors(host.preferences['dark']);normal=QTextCharFormat();normal.setForeground(QColor(colors['text']))
        for day in Qt.DayOfWeek:self.calendar.setWeekdayTextFormat(day,normal)
        if hasattr(host,'control_icon'):
            previous=self.calendar.findChild(QToolButton,'qt_calendar_prevmonth');following=self.calendar.findChild(QToolButton,'qt_calendar_nextmonth');icon=host.control_icon('back')
            if previous:previous.setIcon(icon)
            if following:following.setIcon(QIcon(icon.pixmap(18,18).transformed(QTransform().rotate(180))))
        self.calendar.clicked.connect(self.pick_date);self.calendar_button.clicked.connect(self.open_calendar)
        for kind,field in [('date',self.date),('time',self.time)]:
            field.textChanged.connect(lambda _,k=kind:self.validate(k,False,False));field.editingFinished.connect(lambda k=kind:self.validate(k,True,True))
        self.validate('date',False,False);self.validate('time',False,False)

    def input_box(self,title,field,inline=False,extra=None):
        outer=QWidget();outer_layout=QVBoxLayout(outer);outer_layout.setContentsMargins(0,0,0,0);outer_layout.setSpacing(4)
        frame=QFrame();frame.setObjectName('appointmentField');box=QHBoxLayout(frame) if inline else QVBoxLayout(frame);box.setContentsMargins(10,7,10,7);box.setSpacing(6)
        label=QLabel(title);label.setObjectName('profileCaption');label.setBuddy(field);box.addWidget(label)
        if inline:box.addStretch()
        box.addWidget(field)
        if extra:box.addWidget(extra)
        outer_layout.addWidget(frame);error=QLabel();error.setObjectName('appointmentError');error.setWordWrap(True);error.hide();outer_layout.addWidget(error);field.appointment_frame=frame;field.installEventFilter(self)
        if isinstance(field,QComboBox):field.lineEdit().appointment_frame=frame;field.lineEdit().installEventFilter(self)
        return outer,error

    def eventFilter(self,watched,event):
        frame=getattr(watched,'appointment_frame',None)
        if frame is not None and event.type() in {QEvent.Type.FocusIn,QEvent.Type.FocusOut}:
            frame.setProperty('focused',event.type()==QEvent.Type.FocusIn);frame.style().unpolish(frame);frame.style().polish(frame)
        return super().eventFilter(watched,event)

    def office_selected(self,index):
        parts=self.office.itemData(index)
        if parts:self.office.setCurrentText(parts[0]);self.address.setText(parts[1])

    def validate(self,kind,format_value,show_error):
        field=getattr(self,kind);parser=format_date if kind=='date' else format_time;value=parser(field.text());error=getattr(self,kind+'_error')
        if value and format_value and field.text()!=value:field.setText(value)
        if show_error or value:
            error.setVisible(not bool(value));error.setText(self.host.t('Требуется существующая дата: ДД.ММ.ГГГГ' if kind=='date' else 'Допустимое время: 00:00–23:59'))
            frame=field.appointment_frame;frame.setProperty('invalid',not bool(value));frame.style().unpolish(frame);frame.style().polish(frame)
        self.save.setEnabled(bool(format_date(self.date.text()) and format_time(self.time.text())))
        return bool(value)

    def accept(self):
        date_ok=self.validate('date',True,True);time_ok=self.validate('time',True,True)
        if date_ok and time_ok:super().accept()

    def open_calendar(self):
        value=format_date(self.date.text())
        if value:self.calendar.setSelectedDate(QDate.fromString(value,'dd.MM.yyyy'))
        self.calendar_popup.adjustSize();position=self.calendar_button.mapToGlobal(QPoint(0,self.calendar_button.height()+6));screen=self.screen().availableGeometry();position.setX(min(position.x(),screen.right()-self.calendar_popup.width()));position.setY(min(position.y(),screen.bottom()-self.calendar_popup.height()));self.calendar_popup.move(position);self.calendar_popup.show()

    def pick_date(self,date):
        self.date.setText(date.toString('dd.MM.yyyy'));self.validate('date',True,True);self.calendar_popup.hide()

    def details(self,zone):
        name=self.office.currentText().strip()
        if name in self.parts:name=self.parts[name][0]
        return dict(date=QDate.fromString(format_date(self.date.text()),'dd.MM.yyyy').toString('yyyy-MM-dd'),time=format_time(self.time.text()),office=name,address=self.address.text().strip(),confirmation=self.reference.text().strip(),timezone=zone)
