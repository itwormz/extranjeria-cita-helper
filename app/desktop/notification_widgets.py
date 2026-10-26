"""Profile-scoped notifications, composed from the existing native settings fields."""
import copy
import json
import html
import time
from PySide6.QtCore import Qt, QSize, QTimer, QUrl, Signal
from PySide6.QtGui import QDesktopServices, QPixmap
from PySide6.QtWidgets import (QWidget,QDialog,QVBoxLayout,QHBoxLayout,QGridLayout,QLabel,QLineEdit,
    QPushButton,QToolButton,QCheckBox,QScrollArea,QFrame,QApplication,QPlainTextEdit,QListView,QLayout)
from .profile_widgets import ProfileField
from .notification_jobs import NotificationJobs
from .notification_store import NotificationVault
from ..notifications import (EVENTS,EVENT_LABELS,NotificationError,defaults,settings,Telegram,
    make_event,validate_webhook,webhook_request,send_request)


def text(owner,value,muted=False):
    label=QLabel(owner.t(value));label.setWordWrap(True);label.setTextFormat(Qt.TextFormat.PlainText)
    label.setProperty('notificationSource',value)
    if muted:label.setProperty('muted',True)
    return label


def tool(owner,icon,title):
    button=QToolButton();button.setProperty('toolbarAction',True);button.setFixedSize(28,28)
    button.setProperty('notificationIcon',icon);button.setProperty('notificationTooltip',title)
    button.setIcon(owner.control_icon(icon));button.setIconSize(QSize(17,17))
    button.setAccessibleName(owner.t(title));button.setToolTip(owner.t(title))
    return button


def button(owner,title,icon=None):
    b=QPushButton(owner.t(title));b.setProperty('notificationSource',title)
    if icon:b.setProperty('notificationIcon',icon)
    if icon:b.setIcon(owner.control_icon(icon));b.setIconSize(QSize(17,17))
    return b


def rule():
    line=QFrame();line.setObjectName('notificationDivider');line.setFixedHeight(1);return line


class TextDisclosure(QLabel):
    toggled=Signal(bool)

    def __init__(self,owner,title):
        super().__init__();self.owner=owner;self.title=title;self.expanded=False
        self.setProperty('notificationDisclosure',True)
        self.setTextFormat(Qt.TextFormat.RichText)
        self.setTextInteractionFlags(Qt.TextInteractionFlag.TextBrowserInteraction)
        self.setOpenExternalLinks(False);self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
        self.linkActivated.connect(lambda _:self.setChecked(not self.expanded));self.refresh_text()

    def refresh_text(self):
        title=self.owner.t(self.title)
        from .theme import theme_colors
        color=theme_colors(getattr(self.owner,'preferences',{}).get('dark',False))['accent_text']
        self.setText(f'<a href="#" style="text-decoration:none;color:{color}">'+('▾ ' if self.expanded else '▸ ')+html.escape(title)+'</a>')
        self.setAccessibleName(title)

    def setChecked(self,value):
        self.expanded=bool(value);self.refresh_text();self.toggled.emit(self.expanded)

    def isChecked(self):return self.expanded


def style_notification_combo(combo):
    from .window import ThemedItemDelegate
    view=QListView(combo);view.setItemDelegate(ThemedItemDelegate(view));combo.setView(view)
    combo.setStyleSheet('QComboBox { combobox-popup: 0; }')
    for widget in (combo,view,view.viewport()):widget.setAttribute(Qt.WidgetAttribute.WA_MacShowFocusRect,False)
    popup=view.parentWidget()
    if isinstance(popup,QFrame):popup.setFrameShape(QFrame.Shape.NoFrame);popup.setObjectName('comboPopup')


class ActionField(ProfileField):
    def __init__(self,owner,title,field,icon,actions=()):
        self.actions=[]
        super().__init__(owner.t(title),field)
        self.caption.setProperty('notificationSource',title)
        self.icon.setProperty('notificationFieldIcon',icon)
        self.icon.setPixmap(owner.control_icon(icon).pixmap(QSize(18,18))) if icon else self.icon.hide()
        field.setAttribute(Qt.WidgetAttribute.WA_MacShowFocusRect,False)
        field.setAccessibleName(owner.t(title))
        for action in actions:action.setParent(self);self.actions.append(action)
        self.sync_caption()

    def sync_caption(self,*args):
        super().sync_caption(*args)
        count=len(self.actions)
        left=40 if self.icon.property('notificationFieldIcon') else 12
        self.field.move(left,self.field.y());self.caption.move(left,self.caption.y())
        self.field.resize(max(1,self.width()-left-9-count*28),self.field.height())
        self.caption.resize(max(1,self.width()-left-9-count*28),self.caption.height())
        for index,action in enumerate(self.actions):action.setGeometry(self.width()-9-28*(count-index),10,28,28)


class Events(QWidget):
    def __init__(self,owner):
        super().__init__();self.owner=owner;self.checks={}
        layout=QVBoxLayout(self);layout.setContentsMargins(0,8,0,0);layout.setSpacing(7)
        summary=QHBoxLayout();self.all_events=QCheckBox(owner.t('Все важные события'))
        self.all_events.setObjectName('notificationEvent');self.all_events.setProperty('notificationSource','Все важные события');self.all_events.setTristate(True)
        summary.addWidget(self.all_events);summary.addStretch()
        self.disclosure=TextDisclosure(owner,'Настроить');summary.addWidget(self.disclosure);layout.addLayout(summary)
        self.details=QWidget();grid=QGridLayout(self.details);grid.setContentsMargins(0,3,0,0);grid.setSpacing(7)
        for index,key in enumerate(EVENTS):
            check=QCheckBox(owner.t(EVENT_LABELS[key]));check.setObjectName('notificationEvent');check.setProperty('notificationSource',EVENT_LABELS[key]);check.setChecked(True);self.checks[key]=check
            check.toggled.connect(self.sync);grid.addWidget(check,index//2,index%2)
        grid.setColumnStretch(0,1);grid.setColumnStretch(1,1);self.details.hide();layout.addWidget(self.details)
        self.disclosure.toggled.connect(self.details.setVisible)
        self.note=text(owner,'',True);self.note.setProperty('notificationData',True);layout.addWidget(self.note)
        self.all_events.clicked.connect(self.select_all);self.sync()

    def select_all(self,checked):
        for check in self.checks.values():check.blockSignals(True);check.setChecked(checked);check.blockSignals(False)
        self.sync()

    def sync(self,*args):
        count=sum(check.isChecked() for check in self.checks.values())
        state=Qt.CheckState.Checked if count==len(EVENTS) else Qt.CheckState.PartiallyChecked if count else Qt.CheckState.Unchecked
        self.all_events.setCheckState(state)
        message='Только когда требуется участие или поиск остановлен.' if count==len(EVENTS) else 'Выбрано событий: {count} из {total}.' if count else 'События не выбраны.'
        self.note.setText(self.owner.t(message).format(count=count,total=len(EVENTS)))

    def load(self,values):
        for key,check in self.checks.items():check.blockSignals(True);check.setChecked(key in values);check.blockSignals(False)
        self.disclosure.setChecked(False);self.sync()

    def value(self):return [key for key,check in self.checks.items() if check.isChecked()]


class TestButton(QPushButton):
    """A fixed-size test action: feedback stays inside the originating control."""
    def __init__(self,owner,height=32):
        super().__init__();self.owner=owner;self.state='idle';self.detail='Отправить тест'
        self.requested_height=height;self.setFixedSize(110,height);self.setIconSize(QSize(17,17))
        self.setProperty('notificationData',True);self.setProperty('notificationTestButton',True);self.refresh_text()

    def refresh_text(self):
        title,icon={'idle':('Тест','test'),'sending':('Отправка','clock'),'accepted':('Принято','check'),'error':('Ошибка','alert')}[self.state]
        self.setText(self.owner.t(title));self.setIcon(self.owner.control_icon(icon));self.setToolTip(self.owner.t(self.detail))
        self.setAccessibleName(self.owner.t('Отправить тест')+' · '+self.owner.t(title))
        self.setProperty('notificationTestState',self.state);self.style().unpolish(self);self.style().polish(self)
        self.setFixedSize(110,self.requested_height)

    def start(self,jobs,work,detail,current=lambda:True):
        if self.state=='sending':return
        self.state='sending';self.detail='Выполняется запрос…';self.setEnabled(False);self.refresh_text()
        def done(ok,value):
            from shiboken6 import isValid
            if not isValid(self):return
            self.setEnabled(True)
            if not current():self.state='idle';self.detail='Отправить тест';self.refresh_text();return
            self.state='accepted' if ok else 'error';self.detail=detail(value) if ok else str(value);self.refresh_text()
        jobs.submit(work,done)


class NotificationPanel(QWidget):
    def __init__(self,owner):
        super().__init__();self.owner=owner;self.vault=NotificationVault();self.jobs=NotificationJobs(self)
        self.config=defaults();self.webhook={};self.recipients=[];self.loaded_secrets={};self.load_error='';self.generation=0
        from .window import Toggle,ProfileComboBox
        layout=QVBoxLayout(self);layout.setContentsMargins(8,12,8,12);layout.setSpacing(12)
        self.telegram_toggle=Toggle('Telegram');self.webhook_toggle=Toggle('Webhook')
        for name,toggle in [('Telegram',self.telegram_toggle),('Webhook',self.webhook_toggle)]:
            if name=='Webhook':continue
            heading=QHBoxLayout();heading.addWidget(text(owner,name));heading.addStretch();heading.addWidget(toggle);layout.addLayout(heading)
        self.telegram_body=QWidget();tg=QVBoxLayout(self.telegram_body);tg.setContentsMargins(0,0,0,0);tg.setSpacing(6)
        self.token=QLineEdit();self.token.setMaxLength(256);self.token.setEchoMode(QLineEdit.EchoMode.Password)
        self.eye=tool(owner,'eye','Показать значение');self.eye.clicked.connect(lambda:self.token.setEchoMode(QLineEdit.EchoMode.Normal if self.token.echoMode()==QLineEdit.EchoMode.Password else QLineEdit.EchoMode.Password))
        self.token_box=ActionField(owner,'Токен бота',self.token,'key',[self.eye])
        token_row=QHBoxLayout();token_row.setSpacing(10);token_row.addWidget(self.token_box,1)
        self.check_bot=button(owner,'Проверить','check');self.check_bot.setFixedHeight(48);self.check_bot.clicked.connect(self.verify_bot);token_row.addWidget(self.check_bot);tg.addLayout(token_row)
        self.bot_status=text(owner,'',True);self.bot_status.setProperty('notificationData',True);help_row=QHBoxLayout()
        help_button=TextDisclosure(owner,'Как получить токен');help_button.toggled.connect(lambda value:self.token_help.setVisible(value))
        help_row.addWidget(help_button);help_row.addStretch();help_row.addWidget(self.bot_status);tg.addLayout(help_row)
        self.token_help=text(owner,'Открыть @BotFather в Telegram → /newbot → задать имя бота → скопировать токен.',True);self.token_help.hide();tg.addWidget(self.token_help)
        recipient_row=QHBoxLayout();recipient_row.addWidget(text(owner,'Получатели',True));recipient_row.addStretch()
        add=tool(owner,'plus','Добавить чат');add.clicked.connect(self.add_recipient);recipient_row.addWidget(add);tg.addLayout(recipient_row)
        self.recipient_list=QVBoxLayout();self.recipient_list.setSpacing(0);tg.addLayout(self.recipient_list)
        self.telegram_status=text(owner,'',True);self.telegram_status.setProperty('notificationData',True);tg.addWidget(self.telegram_status)
        self.telegram_events=Events(owner);tg.addWidget(self.telegram_events);layout.addWidget(self.telegram_body)
        layout.addWidget(rule());heading=QHBoxLayout();heading.addWidget(text(owner,'Webhook'));heading.addStretch();heading.addWidget(self.webhook_toggle);layout.addLayout(heading)
        self.webhook_body=QWidget();hook=QVBoxLayout(self.webhook_body);hook.setContentsMargins(0,0,0,0);hook.setSpacing(8)
        self.url=QLineEdit();self.url.setMaxLength(4096);self.method=ProfileComboBox();self.method.addItems(['POST','GET'])
        url_row=QHBoxLayout();url_row.setSpacing(10);self.url_box=ActionField(owner,'URL получателя',self.url,'external');url_row.addWidget(self.url_box,1)
        style_notification_combo(self.method)
        self.method_box=ActionField(owner,'Метод',self.method,None);self.method_box.setFixedWidth(120);url_row.addWidget(self.method_box)
        self.test_webhook=TestButton(owner,height=36);self.test_webhook.clicked.connect(lambda:self.test('webhook'));hook.addLayout(url_row)
        self.method_hint=text(owner,'Событие отправляется в теле запроса в формате JSON.',True);hook.addWidget(self.method_hint)
        self.method.currentTextChanged.connect(lambda value:self.method_hint.setText(owner.t('Без тела запроса. Данные передаются параметрами URL.' if value=='GET' else 'Событие отправляется в теле запроса в формате JSON.')))
        self.webhook_status=text(owner,'',True);self.webhook_status.setProperty('notificationData',True);hook.addWidget(self.webhook_status)
        advanced=button(owner,'Дополнительные параметры','settings');advanced.setProperty('toolbarAction',True);advanced.clicked.connect(self.edit_webhook);hook.addWidget(advanced,alignment=Qt.AlignmentFlag.AlignLeft)
        self.webhook_events=Events(owner);hook.addWidget(self.webhook_events);hook.addWidget(self.test_webhook,alignment=Qt.AlignmentFlag.AlignLeft);layout.addWidget(self.webhook_body);layout.addStretch()
        self.telegram_toggle.toggled.connect(self.telegram_body.setVisible);self.webhook_toggle.toggled.connect(self.webhook_body.setVisible)
        self.token.textChanged.connect(lambda:self.bot_status.setText(''))
        self.scroll=QScrollArea();self.scroll.setWidgetResizable(True);self.scroll.setFrameShape(QScrollArea.Shape.NoFrame);self.scroll.setWidget(self)
        self.load({})

    def retranslate(self):
        for check in self.findChildren(QCheckBox):
            source=check.property('notificationSource')
            if source:check.setText(self.owner.t(source))
        self.telegram_events.sync();self.webhook_events.sync()
        for edit in self.findChildren(QLineEdit):
            box=edit.parentWidget()
            if isinstance(box,ActionField):edit.setPlaceholderText(self.owner.t(box.caption.property('notificationSource') or ''))

    def ensure_secrets_loaded(self):
        if not self.secrets_loaded:self.load(self.config)

    def load(self,raw,*,defer_secrets=False):
        self.generation+=1;self.config=settings(raw);self.loaded_secrets={};self.load_error=''
        self.secrets_loaded=not defer_secrets
        for channel in ('telegram','webhook'):
            if defer_secrets:
                self.loaded_secrets[channel]={}
                continue
            try:self.loaded_secrets[channel]=self.vault.get(self.config[channel].get('secret_ref',''))
            except NotificationError as exc:self.loaded_secrets[channel]={};self.load_error=str(exc)
        self.token.setText(self.loaded_secrets['telegram'].get('token',''));self.token.setEchoMode(QLineEdit.EchoMode.Password)
        self.webhook=copy.deepcopy(self.loaded_secrets['webhook']);self.url.setText(self.webhook.get('url',''));self.method.setCurrentText(self.webhook.get('method','POST'))
        self.recipients=copy.deepcopy(self.config['telegram'].get('recipients',[]));self.render_recipients()
        for name in ('telegram','webhook'):
            getattr(self,name+'_toggle').setChecked(bool(self.config[name]['enabled']))
            getattr(self,name+'_body').setVisible(bool(self.config[name]['enabled']))
            getattr(self,name+'_events').load(self.config[name].get('events',list(EVENTS)))
            getattr(self,name+'_status').setText(self.owner.t(self.load_error))
            getattr(self,name+'_status').setVisible(bool(self.load_error))

    def render_recipients(self):
        while self.recipient_list.count():
            item=self.recipient_list.takeAt(0)
            if item.widget():item.widget().hide();item.widget().deleteLater()
        if not self.recipients:self.recipient_list.addWidget(text(self.owner,'Добавить получателя кнопкой «+».',True))
        for recipient in self.recipients:
            widget=QWidget();row=QHBoxLayout(widget);row.setContentsMargins(0,0,0,0)
            user_icon=QLabel();user_icon.setProperty('notificationFieldIcon','user');user_icon.setPixmap(self.owner.control_icon('user').pixmap(QSize(17,17)));row.addWidget(user_icon)
            names=QHBoxLayout();names.setSpacing(10);name_label=text(self.owner,'');name_label.setProperty('notificationData',True);name_label.setText(recipient['label']);names.addWidget(name_label)
            id_label=text(self.owner,'',True);id_label.setProperty('notificationData',True);id_label.setText('· '+recipient['id']);names.addWidget(id_label);names.addStretch();row.addLayout(names,1)
            test=TestButton(self.owner);test.setProperty('recipientId',recipient['id'])
            test.setToolTip(self.owner.t('Отправить тест этому получателю'));test.clicked.connect(lambda checked=False,r=copy.deepcopy(recipient),b=test:self.test_recipient(r,b));row.addWidget(test)
            remove=tool(self.owner,'trash','Удалить получателя');remove.clicked.connect(lambda checked=False,r=recipient:self.remove_recipient(r));row.addWidget(remove);self.recipient_list.addWidget(widget)

    def remove_recipient(self,recipient):self.recipients.remove(recipient);self.render_recipients()

    def add_recipient(self):
        try:Telegram(self.token.text().strip())
        except NotificationError as exc:self.telegram_status.show();self.telegram_status.setText(self.owner.t(str(exc)));return
        dialog=RecipientDialog(self.owner,self.token.text().strip())
        if dialog.exec()==QDialog.DialogCode.Accepted:
            if not any(r['id']==dialog.recipient['id'] for r in self.recipients):self.recipients.append(dialog.recipient)
            self.render_recipients()

    def run(self,channel,work,success,control):
        status=getattr(self,channel+'_status');status.show();status.setText(self.owner.t('Выполняется запрос…'));control.setEnabled(False);generation=self.generation
        def done(ok,value):
            from shiboken6 import isValid
            if not isValid(control):return
            control.setEnabled(True)
            if generation!=self.generation:return
            status.setText(self.owner.t(success(value) if ok else value))
        self.jobs.submit(work,done)

    def verify_bot(self):
        token=self.token.text().strip()
        def done(value):
            if self.token.text().strip()==token:self.bot_status.setText('@'+value['username'])
            if self.token.text().strip()==token:self.telegram_status.hide()
            return '' if self.token.text().strip()==token else 'Проверить токен бота'
        self.run('telegram',lambda:Telegram(token).check(),done,self.check_bot)

    def hook_config(self):return dict(self.webhook,url=self.url.text().strip(),method=self.method.currentText())

    def test_recipient(self,recipient,control):
        token=self.token.text().strip()
        event=make_event('test',self.owner.fields['label'].text().strip(),self.owner.fields['province'].currentText(),self.owner.preferences['language'])
        generation=self.generation
        control.start(self.jobs,lambda:Telegram(token).send(recipient['id'],event['message']),
                      lambda result:'Сообщение принято Telegram',lambda:generation==self.generation)

    def test(self,channel):
        event=make_event('test',self.owner.fields['label'].text().strip(),self.owner.fields['province'].currentText(),self.owner.preferences['language'])
        config=self.hook_config()
        generation=self.generation
        self.test_webhook.start(self.jobs,lambda:send_request(webhook_request(config,event),attempts=1),lambda result:f'HTTP {result[0]} · {result[2]} ms',lambda:generation==self.generation)

    def edit_webhook(self):
        dialog=WebhookDialog(self.owner,self.hook_config())
        if dialog.exec()==QDialog.DialogCode.Accepted:self.webhook=dialog.value()

    def prepare(self):
        """Create fresh entries before YAML replace; caller commits or rolls back refs."""
        config=copy.deepcopy(self.config);created=[];old=[]
        if not self.secrets_loaded:return config,created,old
        if self.load_error:raise NotificationError(self.load_error)
        try:
            token=self.token.text().strip();hook=self.hook_config()
            if self.telegram_toggle.isChecked():
                Telegram(token)
                if not self.recipients:raise NotificationError('Добавить получателя кнопкой «+».')
            if self.webhook_toggle.isChecked():validate_webhook(hook)
            for channel,secret in [('telegram',{'token':token} if token else {}),('webhook',hook if hook.get('url') else {})]:
                entry=config[channel];entry['enabled']=getattr(self,channel+'_toggle').isChecked();entry['events']=getattr(self,channel+'_events').value()
                if channel=='telegram':entry['recipients']=copy.deepcopy(self.recipients)
                if secret!=self.loaded_secrets.get(channel,{}):
                    reference=self.vault.put(secret) if secret else ''
                    if reference:created.append(reference)
                    if entry.get('secret_ref'):old.append(entry['secret_ref'])
                    entry['secret_ref']=reference
            return config,created,old
        except Exception:
            for reference in created:self.vault.delete(reference)
            raise


class HeaderRow(QWidget):
    def __init__(self,owner,data,headers):
        super().__init__();self.name=QLineEdit(data.get('name',''));self.value=QLineEdit(data.get('value',''))
        self.plus=tool(owner,'plus','Добавить строку');self.remove=tool(owner,'trash','Удалить строку')
        self.secret=tool(owner,'lock','Скрывать значение');self.secret.setCheckable(True);self.secret.setChecked(data.get('secret',True))
        self.eye=tool(owner,'eye','Показать значение')
        actions=[self.secret,self.eye]
        self.name_box=ActionField(owner,'Название',self.name,None);self.value_box=ActionField(owner,'Значение',self.value,None,actions)
        row=QHBoxLayout(self);row.setContentsMargins(0,0,0,0);row.setSpacing(8);row.addWidget(self.name_box,1);row.addWidget(self.value_box,1)
        row.addWidget(self.plus);row.addWidget(self.remove)
        self.secret.toggled.connect(self.mask);self.eye.clicked.connect(self.reveal);self.mask()
        for edit in (self.name,self.value):edit.textChanged.connect(self.update_delete)
        self.update_delete()

    def update_delete(self):self.remove.setEnabled(bool(self.name.text() or self.value.text()) or bool(self.property('multiple')))

    def mask(self,*args):
        self.value.setEchoMode(QLineEdit.EchoMode.Password if self.secret.isChecked() else QLineEdit.EchoMode.Normal);self.eye.setVisible(self.secret.isChecked())

    def reveal(self):self.value.setEchoMode(QLineEdit.EchoMode.Normal if self.value.echoMode()==QLineEdit.EchoMode.Password else QLineEdit.EchoMode.Password)

    def value_dict(self):return dict(name=self.name.text().strip(),value=self.value.text(),secret=self.secret.isChecked())


class Rows(QWidget):
    def __init__(self,owner,values,headers=False):
        super().__init__();self.owner=owner;self.headers=headers;self.rows=[];self.layout=QVBoxLayout(self);self.layout.setContentsMargins(0,0,0,0);self.layout.setSpacing(8)
        for value in values or [{}]:self.add(value)

    def add(self,value=None):
        row=HeaderRow(self.owner,value or {},self.headers);self.rows.append(row);self.layout.addWidget(row)
        row.plus.clicked.connect(lambda:self.add());row.remove.clicked.connect(lambda:self.remove(row));self.refresh()

    def remove(self,row):
        if len(self.rows)==1:row.name.clear();row.value.clear();row.secret.setChecked(True)
        else:self.rows.remove(row);self.layout.removeWidget(row);row.deleteLater()
        self.refresh()

    def refresh(self):
        from PySide6.QtWidgets import QSizePolicy
        for index,row in enumerate(self.rows):
            policy=row.plus.sizePolicy();policy.setRetainSizeWhenHidden(True);row.plus.setSizePolicy(policy)
            row.plus.setVisible(index==len(self.rows)-1);row.setProperty('multiple',len(self.rows)>1);row.update_delete()

    def value(self):return [row.value_dict() for row in self.rows if row.name.text() or row.value.text()]


class WebhookDialog(QDialog):
    def __init__(self,owner,config):
        super().__init__(owner);self.owner=owner;self.original=copy.deepcopy(config);self.setWindowTitle(owner.t('Дополнительные параметры'));self.resize(690,530)
        from .window import ProfileComboBox
        outer=QVBoxLayout(self);outer.setContentsMargins(24,20,24,20);outer.setSpacing(12)
        page=QWidget();layout=QVBoxLayout(page);layout.setContentsMargins(0,0,0,0);layout.setSpacing(10)
        layout.addWidget(text(owner,'Заголовки'));self.headers=Rows(owner,config.get('headers',[]),True);layout.addWidget(self.headers)
        layout.addWidget(text(owner,'Все значения скрыты по умолчанию. Замок снимает скрытие. Значения заголовков не включаются в диагностику.',True));layout.addWidget(rule())
        layout.addWidget(text(owner,'Параметры URL'));self.query=Rows(owner,config.get('query',[]));layout.addWidget(self.query);layout.addWidget(rule())
        self.body_mode=ProfileComboBox()
        style_notification_combo(self.body_mode)
        self.body_mode.addItem(owner.t('Стандартный JSON'),'standard');self.body_mode.addItem(owner.t('Свой JSON'),'custom')
        self.body_mode.setCurrentIndex(max(0,self.body_mode.findData(config.get('body_mode','standard'))))
        self.body_field=ActionField(owner,'Тело запроса',self.body_mode,None);self.body_field.setFixedWidth(260);layout.addWidget(self.body_field)
        self.schema_button=TextDisclosure(owner,'Показать схему JSON');layout.addWidget(self.schema_button,alignment=Qt.AlignmentFlag.AlignLeft)
        self.schema=text(owner,'');self.schema.setProperty('notificationData',True);self.schema.setObjectName('notificationSchema');self.schema.setText(json.dumps(make_event('test','Demo','Barcelona',owner.preferences['language']),ensure_ascii=False,indent=2));self.schema.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse);self.schema.hide();layout.addWidget(self.schema)
        self.schema_button.toggled.connect(self.schema.setVisible)
        self.body=QPlainTextEdit(config.get('body','{\n  "text": "{{message}}"\n}'));self.body.setMinimumHeight(110);layout.addWidget(self.body)
        self.variables=text(owner,'Переменные: {{message}}, {{event}}, {{profile}}, {{province}}, {{timestamp}}, {{event_id}}',True);layout.addWidget(self.variables)
        self.body_mode.currentIndexChanged.connect(self.sync_body);self.sync_body()
        layout.addWidget(text(owner,'Таймаут: 10 с. Ограниченные повторы при сетевых ошибках, 429 и 5xx.',True));layout.addStretch()
        scroll=QScrollArea();scroll.setWidgetResizable(True);scroll.setFrameShape(QScrollArea.Shape.NoFrame);scroll.setWidget(page);outer.addWidget(scroll)
        self.error=text(owner,'',True);outer.addWidget(self.error)
        actions=QHBoxLayout();self.test_button=TestButton(owner,height=36);self.test_button.clicked.connect(self.test);actions.addWidget(self.test_button);actions.addStretch();cancel=button(owner,'Отмена');cancel.clicked.connect(self.reject);actions.addWidget(cancel)
        done=button(owner,'Готово','check');done.setProperty('primary',True);done.clicked.connect(self.save);actions.addWidget(done);outer.addLayout(actions)

    def sync_body(self):
        custom=self.body_mode.currentData()=='custom';post=self.original.get('method','POST')=='POST'
        self.body_field.setVisible(post);self.body.setVisible(post and custom);self.variables.setVisible(post and custom)
        self.schema_button.setVisible(post and not custom);self.schema.setVisible(post and not custom and self.schema_button.isChecked())

    def value(self):return dict(self.original,headers=self.headers.value(),query=self.query.value(),body_mode=self.body_mode.currentData(),body=self.body.toPlainText())

    def test(self):
        config=self.value();event=make_event('test',self.owner.fields['label'].text().strip(),self.owner.fields['province'].currentText(),self.owner.preferences['language'])
        self.test_button.start(self.owner.notifications_panel.jobs,lambda:send_request(webhook_request(config,event),attempts=1),lambda result:f'HTTP {result[0]} · {result[2]} ms')

    def save(self):
        try:validate_webhook(self.value())
        except NotificationError as exc:self.error.setText(self.owner.t(str(exc)));return
        self.accept()


class RecipientChoice(QPushButton):
    def __init__(self,owner):
        super().__init__();self.owner=owner;self.setCheckable(True);self.setMinimumHeight(72)
        self.setProperty('recipientChoice',True);self.setProperty('notificationData',True)
        row=QHBoxLayout(self);row.setContentsMargins(12,10,12,10);row.setSpacing(12)
        self.icon=QLabel();self.icon.setFixedSize(20,20);row.addWidget(self.icon)
        names=QVBoxLayout();names.setSpacing(3)
        self.title=text(owner,'');self.title.setProperty('notificationData',True)
        self.details=text(owner,'',True);self.details.setProperty('notificationData',True)
        names.addWidget(self.title);names.addWidget(self.details);row.addLayout(names,1)
        self.indicator=QLabel();self.indicator.setObjectName('recipientChoiceIndicator');self.indicator.setFixedSize(20,20)
        self.indicator.setAlignment(Qt.AlignmentFlag.AlignCenter);row.addWidget(self.indicator)
        for child in self.findChildren(QWidget):child.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self.toggled.connect(self.mark)

    def set_recipient(self,recipient):
        kind=recipient.get('type','private')
        title={'private':'Личный чат','group':'Группа','supergroup':'Группа','channel':'Канал'}.get(kind,'Чат')
        details=[('@'+recipient['username']) if recipient.get('username') else '',self.owner.t(title),'ID '+recipient['id']]
        self.title.setText(recipient['label']);self.details.setText(' · '.join(v for v in details if v))
        self.icon.setPixmap(self.owner.control_icon('user' if kind=='private' else 'office').pixmap(QSize(20,20)))
        self.setAccessibleName(recipient['label']+' · '+self.details.text())
        self.setChecked(False);self.mark(False)

    def mark(self,checked):
        self.indicator.setPixmap(self.owner.control_icon('check').pixmap(QSize(13,13)) if checked else QPixmap())
        self.indicator.setProperty('selected',checked);self.indicator.style().unpolish(self.indicator);self.indicator.style().polish(self.indicator)


class RecipientDialog(QDialog):
    """Separate connection modes; discovery never silently selects a recipient."""
    _polling_tokens=set()

    def __init__(self,owner,token):
        super().__init__(owner);self.owner=owner;self.token=token;self.client=Telegram(token)
        self.recipient=None;self.candidate=None;self.pair=None;self.kind='private';self.mode=None
        self.closed=False;self.busy=False;self.owns_poll=False;self.release_pending=False;self.serial=0;self.jobs=owner.notifications_panel.jobs
        self.setWindowTitle(owner.t('Добавить получателя'));self.resize(560,430)
        layout=QVBoxLayout(self);layout.setContentsMargins(24,20,24,20);layout.setSpacing(12)
        layout.setSizeConstraint(QLayout.SizeConstraint.SetMinimumSize)
        method_row=QHBoxLayout();self.method_buttons={}
        for mode,title,icon in [('telegram','Через Telegram','send'),('manual','По Chat ID','id')]:
            b=button(owner,title,icon);b.setProperty('profileMode',True);b.setCheckable(True)
            b.clicked.connect(lambda checked,m=mode:self.set_mode(m));method_row.addWidget(b);self.method_buttons[mode]=b
        method_row.addStretch();layout.addLayout(method_row)
        self.auto=QWidget();auto=QVBoxLayout(self.auto);auto.setContentsMargins(0,0,0,0);auto.setSpacing(10)
        kind_row=QHBoxLayout();self.kind_buttons={}
        for kind,title,icon in [('private','Личный чат','user'),('group','Группа','office')]:
            b=button(owner,title,icon);b.setProperty('profileMode',True);b.setCheckable(True);b.setChecked(kind=='private')
            b.clicked.connect(lambda checked,k=kind:self.set_kind(k));kind_row.addWidget(b);self.kind_buttons[kind]=b
        kind_row.addStretch();auto.addLayout(kind_row)
        self.help=text(owner,'Открыть бота в Telegram и нажать «Начать».',True);auto.addWidget(self.help)
        self.pairing_actions=QWidget();actions=QHBoxLayout(self.pairing_actions);actions.setContentsMargins(0,0,0,0)
        self.open_button=button(owner,'Открыть бота','external');self.open_button.clicked.connect(lambda:self.start_pairing(False));actions.addWidget(self.open_button)
        self.link_button=button(owner,'Скопировать ссылку','copy');self.link_button.clicked.connect(lambda:self.start_pairing(True));actions.addWidget(self.link_button);actions.addStretch();auto.addWidget(self.pairing_actions)
        self.link_hint=text(owner,'Для другого получателя — отправить ему ссылку. Она действует 2 минуты.',True);auto.addWidget(self.link_hint);layout.addWidget(self.auto)
        self.manual=QWidget();manual=QVBoxLayout(self.manual);manual.setContentsMargins(0,0,0,0);manual.setSpacing(10)
        manual.addWidget(text(owner,'Указать Chat ID. Для публичного канала — @имя_канала.',True))
        self.chat_id=QLineEdit();self.chat_id.setMaxLength(128);input_row=QHBoxLayout()
        input_row.addWidget(ActionField(owner,'Chat ID или @канал',self.chat_id,'id'),1)
        self.check_manual_button=button(owner,'Проверить','check');self.check_manual_button.setFixedHeight(48)
        self.check_manual_button.clicked.connect(self.check_manual);input_row.addWidget(self.check_manual_button);manual.addLayout(input_row)
        self.manual_hint=text(owner,'Боту нужен доступ к чату. Для канала — право публикации.',True);manual.addWidget(self.manual_hint);layout.addWidget(self.manual)
        layout.addWidget(rule());self.status=text(owner,'',True);self.status.setProperty('notificationData',True);layout.addWidget(self.status)
        self.candidate_button=RecipientChoice(owner);self.candidate_button.hide();self.candidate_button.toggled.connect(self.choose);layout.addWidget(self.candidate_button)
        layout.addStretch()
        actions=QHBoxLayout();actions.addStretch();cancel=button(owner,'Отмена');cancel.clicked.connect(self.reject);actions.addWidget(cancel)
        self.add_button=button(owner,'Добавить','plus');self.add_button.setProperty('primary',True);self.add_button.setEnabled(False);self.add_button.clicked.connect(self.confirm);actions.addWidget(self.add_button);layout.addLayout(actions)
        self.timer=QTimer(self);self.timer.setInterval(2500);self.timer.timeout.connect(self.poll);self.finished.connect(self.stop)
        self.chat_id.textChanged.connect(self.manual_changed);self.set_mode('telegram')

    def stop(self,*args):
        self.closed=True;self.serial+=1;self.timer.stop();self.release()

    def release(self):
        if not self.owns_poll:return
        if self.busy:self.release_pending=True;return
        self._polling_tokens.discard(self.token);self.owns_poll=False;self.release_pending=False

    def invalidate(self):
        self.serial+=1;self.recipient=None;self.candidate=None;self.candidate_button.setChecked(False)
        self.candidate_button.hide();self.add_button.setEnabled(False)

    def set_mode(self,mode):
        if self.mode==mode:return
        self.invalidate();self.timer.stop();self.pair=None;self.release();self.mode=mode
        for key,b in self.method_buttons.items():b.setChecked(key==mode)
        self.auto.setVisible(mode=='telegram');self.manual.setVisible(mode=='manual')
        self.pairing_actions.show();self.link_hint.show()
        self.status.setText(self.owner.t('Ожидание сообщения боту' if mode=='telegram' else 'Сначала проверить Chat ID.'))

    def set_kind(self,kind):
        if self.kind==kind:return
        self.invalidate();self.kind=kind;self.timer.stop();self.pair=None;self.release()
        for key,b in self.kind_buttons.items():b.setChecked(key==kind)
        self.pairing_actions.show();self.link_hint.show()
        self.help.setText(self.owner.t('Выбрать группу в Telegram и добавить бота по ссылке.' if kind=='group' else 'Открыть бота в Telegram и нажать «Начать».'))
        self.open_button.setText(self.owner.t('Добавить бота в группу' if kind=='group' else 'Открыть бота'))
        self.status.setText(self.owner.t('Ожидание сообщения боту'))

    def manual_changed(self,*args):
        if self.mode=='manual':self.invalidate();self.status.setText(self.owner.t('Сначала проверить Chat ID.'))

    def request(self,work,callback):
        serial=self.serial;self.busy=True
        for b in (self.open_button,self.link_button,self.check_manual_button):b.setEnabled(False)
        def done(ok,value):
            self.busy=False
            if self.release_pending:self.release()
            if self.closed:return
            for b in (self.open_button,self.link_button,self.check_manual_button):b.setEnabled(True)
            if serial!=self.serial:return
            if ok:callback(value)
            else:self.timer.stop();self.release();self.status.setText(self.owner.t(value))
        self.jobs.submit(work,done)

    def start_pairing(self,copy_link):
        if self.busy:return
        if self.pair and time.monotonic()-self.pair.created<120:self.open_link(copy_link);return
        if self.token in self._polling_tokens:self.status.setText(self.owner.t('Подключение этого бота уже открыто в другом окне.'));return
        self._polling_tokens.add(self.token);self.owns_poll=True
        def ready(pair):self.pair=pair;self.open_link(copy_link);self.timer.start();self.poll()
        self.status.setText(self.owner.t('Выполняется запрос…'));self.request(self.client.begin_pairing,ready)

    def open_link(self,copy_link):
        link=self.pair.link(self.kind)
        if copy_link:QApplication.clipboard().setText(link)
        else:QDesktopServices.openUrl(QUrl(link))
        self.status.setText(self.owner.t('Ожидание сообщения боту'))

    def poll(self):
        if self.busy or not self.pair:return
        pair=self.pair;kind=self.kind
        self.request(lambda:self.client.poll(pair,kind),lambda result:self.found(result) if result else None)

    def found(self,recipient):
        self.timer.stop();self.release();self.recipient=None;self.candidate=copy.deepcopy(recipient)
        self.candidate_button.set_recipient(recipient);self.candidate_button.show();self.add_button.setEnabled(False)
        self.status.setText(self.owner.t('Найден чат — выбрать получателя'))
        if self.mode=='telegram':
            self.pairing_actions.hide();self.link_hint.hide();self.help.setText(self.owner.t('Получен ответ по ссылке подключения.'))

    def choose(self,selected):
        self.recipient=copy.deepcopy(self.candidate) if selected else None
        self.add_button.setEnabled(bool(self.recipient))
        if self.candidate:self.status.setText(self.owner.t('Получатель выбран' if selected else 'Найден чат — выбрать получателя'))

    def check_manual(self):
        if self.busy:return
        self.invalidate();self.timer.stop();self.pair=None;self.release();chat_id=self.chat_id.text().strip()
        self.status.setText(self.owner.t('Выполняется запрос…'));self.request(lambda:self.client.chat(chat_id),self.found)

    def confirm(self):
        if self.recipient:self.accept()
