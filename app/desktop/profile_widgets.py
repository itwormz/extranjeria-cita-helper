"""Compact profile inputs with persistent labels once populated."""
from PySide6.QtCore import Qt, QEvent
from PySide6.QtWidgets import QWidget, QLabel, QLineEdit, QHBoxLayout


class ProfileField(QWidget):
    def __init__(self, title, field):
        super().__init__()
        self.setObjectName('profileField')
        self.setAttribute(Qt.WidgetAttribute.WA_StyledBackground,True)
        self.setFixedHeight(48)
        self.field=field; field.setParent(self)
        self.icon=QLabel(self); self.icon.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self.caption=QLabel(title,self); self.caption.setObjectName('profileCaption')
        self.caption.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        field.installEventFilter(self)
        if isinstance(field,QLineEdit):
            field.setPlaceholderText(title)
            field.textChanged.connect(self.sync_caption)
        else:
            if field.isEditable():field.lineEdit().installEventFilter(self)
        self.sync_caption()

    def sync_caption(self, *_):
        populated=not isinstance(self.field,QLineEdit) or bool(self.field.text())
        self.caption.setVisible(populated)
        self.field.setGeometry(40,20 if populated else 6,max(1,self.width()-49),24 if populated else 36)
        self.caption.setGeometry(40,5,max(1,self.width()-49),15)
        self.icon.setGeometry(12,15,18,18)

    def resizeEvent(self,event):
        super().resizeEvent(event);self.sync_caption()

    def eventFilter(self,watched,event):
        if event.type() in (QEvent.Type.FocusIn,QEvent.Type.FocusOut):
            self.setProperty('focused',event.type()==QEvent.Type.FocusIn)
            self.style().unpolish(self);self.style().polish(self)
        return super().eventFilter(watched,event)


class TimingField(QWidget):
    def __init__(self,title,fields,unit):
        super().__init__()
        self.setObjectName('timingField');self.setAttribute(Qt.WidgetAttribute.WA_StyledBackground,True)
        self.setFixedHeight(48)
        self.icon=QLabel(self);self.icon.setGeometry(10,15,17,17)
        self.caption=QLabel(title,self);self.caption.setObjectName('profileCaption')
        self.help=QLabel(self);self.help.setToolTip(title)
        self.values=QWidget(self);row=QHBoxLayout(self.values);row.setContentsMargins(0,0,0,0);row.setSpacing(5)
        for index,field in enumerate(fields):
            if index:row.addWidget(QLabel('—'))
            field.setMinimumWidth(0);field.setFixedHeight(22)
            if len(fields)==2:field.setMaximumWidth(65)
            row.addWidget(field,1)
        unit_label=QLabel(unit);unit_label.setObjectName('profileCaption');row.addWidget(unit_label)
        row.addStretch(1 if len(fields)==2 else 0)

    def resizeEvent(self,event):
        super().resizeEvent(event)
        self.caption.setGeometry(34,4,max(1,self.width()-60),16)
        self.values.setGeometry(34,23,max(1,self.width()-64),22)
        self.help.setGeometry(self.width()-23,15,15,15)
