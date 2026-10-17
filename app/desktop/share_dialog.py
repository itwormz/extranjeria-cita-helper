"""Prepare a local text or PNG post; never publish or upload automatically."""
from pathlib import Path
from PySide6.QtCore import Qt, QUrl
from PySide6.QtGui import QDesktopServices,QPixmap,QIcon,QPainter,QColor
from PySide6.QtWidgets import (QDialog,QVBoxLayout,QHBoxLayout,QPushButton,QLabel,
    QStackedWidget,QTextEdit,QButtonGroup,QFrame,QApplication)
from .theme import theme_colors
from .achievement_exports import public_post,render_card,share_url

NETWORK_NAMES={'telegram':'Telegram','x':'X','facebook':'Facebook','instagram':'Instagram'}


class ShareDialog(QDialog):
    def __init__(self,host,achievement,network):
        super().__init__(host);self.host=host;self.achievement=achievement;self.network=network;self.mode='text';self.prepared_path=None;t=host.t;self.network_name=NETWORK_NAMES[network]
        self.setWindowTitle(t('Поделиться достижением')+' — '+self.network_name);self.resize(600,540);self.setMinimumWidth(520)
        layout=QVBoxLayout(self);layout.setContentsMargins(22,20,22,20);layout.setSpacing(12)
        modes=QHBoxLayout();group=QButtonGroup(self);group.setExclusive(True);self.buttons={}
        for mode,title,icon in [('text','Текст','text'),('card','Карточка','calendar')]:
            button=QPushButton(t(title));button.setCheckable(True);button.setProperty('shareMode',True)
            if hasattr(host,'control_icon'):button.setIcon(host.control_icon(icon))
            group.addButton(button);modes.addWidget(button);self.buttons[mode]=button;button.clicked.connect(lambda checked=False,m=mode:self.select_mode(m))
        modes.addStretch();layout.addLayout(modes);self.stack=QStackedWidget();layout.addWidget(self.stack,1)
        self.text=QTextEdit();self.text.setObjectName('shareText');self.text.setPlainText(public_post(achievement,host.preferences['language'],detailed=True));self.text.setMinimumHeight(280);self.stack.addWidget(self.text)
        self.image=render_card(achievement,host.preferences['language'],host.preferences['dark']);self.preview=QLabel();self.preview.setAlignment(Qt.AlignmentFlag.AlignCenter);self.preview.setMinimumHeight(280);self.stack.addWidget(self.preview)
        self.help=QLabel();self.help.setProperty('muted',True);self.help.setWordWrap(True);layout.addWidget(self.help)
        privacy=QLabel(t('Только статистика — без данных записи.'));privacy.setProperty('muted',True);layout.addWidget(privacy)
        actions=QHBoxLayout();close=QPushButton(t('Закрыть'));close.clicked.connect(self.reject);actions.addWidget(close);actions.addStretch();self.prepare_button=QPushButton(t('Подготовить и открыть')+' '+self.network_name);self.prepare_button.setProperty('primary',True);self.prepare_button.clicked.connect(self.prepare);actions.addWidget(self.prepare_button);layout.addLayout(actions)
        self.result=QFrame();self.result.setObjectName('shareResult');result_layout=QVBoxLayout(self.result);result_layout.setContentsMargins(12,10,12,10);result_layout.setSpacing(8);self.result_title=QLabel();self.result_title.setObjectName('shareResultTitle');result_layout.addWidget(self.result_title);self.result_body=QLabel();self.result_body.setWordWrap(True);result_layout.addWidget(self.result_body)
        file_row=QHBoxLayout();self.file_name=QLabel();self.file_name.setProperty('muted',True);file_row.addWidget(self.file_name,1);self.show_file=QPushButton(t('Показать файл'));self.show_file.clicked.connect(self.open_folder);file_row.addWidget(self.show_file);result_layout.addLayout(file_row);layout.addWidget(self.result);self.result.hide();self.select_mode('text')

    def select_mode(self,mode):
        self.mode=mode;self.buttons[mode].setChecked(True)
        if hasattr(self.host,'control_icon'):
            for name,button in self.buttons.items():
                icon=self.host.control_icon('text' if name=='text' else 'calendar')
                if name==mode:
                    pixmap=icon.pixmap(18,18);painter=QPainter(pixmap);painter.setCompositionMode(QPainter.CompositionMode.CompositionMode_SourceIn);painter.fillRect(pixmap.rect(),QColor(theme_colors(self.host.preferences['dark'])['on_brand']));painter.end();icon=QIcon(pixmap)
                button.setIcon(icon)
        self.stack.setCurrentIndex(0 if mode=='text' else 1);self.result.hide()
        self.help.setText(self.host.t('Скопируем текст и откроем соцсеть. Останется вставить его в пост.' if mode=='text' else 'Сохраним PNG и откроем соцсеть. Останется прикрепить файл к посту.'));self.update_preview()

    def resizeEvent(self,event):
        super().resizeEvent(event)
        if hasattr(self,'preview'):self.update_preview()

    def update_preview(self):
        self.preview.setPixmap(QPixmap.fromImage(self.image).scaled(max(1,self.width()-60),300,Qt.AspectRatioMode.KeepAspectRatio,Qt.TransformationMode.SmoothTransformation))

    def prepare(self):
        t=self.host.t
        if self.mode=='text':
            QApplication.clipboard().setText(self.text.toPlainText());self.result_title.setText(t('Текст скопирован'));self.result_body.setText(t('Вставить текст в публикацию и подтвердить отправку. Если требуется фото — прикрепить карточку.'));self.file_name.hide();self.show_file.hide()
        else:
            directory=self.host.store.path.parent/'data'/'share'
            try:
                directory.mkdir(parents=True,exist_ok=True,mode=0o700);path=directory/('cita-achievement-'+self.achievement['id']+'-'+self.host.preferences['language']+'.png')
                if not self.image.save(str(path),'PNG'):raise OSError('PNG export failed')
            except OSError:
                self.result_title.setText(t('Не удалось сохранить картинку'));self.result_body.setText(t('Повторить попытку.'));self.file_name.hide();self.show_file.hide();self.result.show();return
            self.prepared_path=path;self.result_title.setText(t('Карточка подготовлена'));self.result_body.setText(t('Создать публикацию, прикрепить подготовленный PNG и подтвердить отправку.'));self.file_name.setText('cita-achievement.png');self.file_name.setToolTip(str(path));self.file_name.show();self.show_file.show()
        self.result.show();url=share_url(self.network,self.text.toPlainText())
        if url and not QDesktopServices.openUrl(QUrl(url)):self.result_body.setText(t('Открыть соцсеть вручную. Подготовленный текст или картинка уже доступны.'))

    def open_folder(self):
        if self.prepared_path:QDesktopServices.openUrl(QUrl.fromLocalFile(str(self.prepared_path.parent)))
