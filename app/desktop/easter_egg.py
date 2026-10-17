"""A local, silent appointment chase hidden in About."""
from PySide6.QtCore import Qt,QPoint,Signal
from PySide6.QtWidgets import QLabel,QDialog,QPushButton,QVBoxLayout


class VersionLabel(QLabel):
    activated=Signal()
    def __init__(self,text):
        super().__init__(text)
        self.clicks=0
    def mouseReleaseEvent(self,event):
        if event.button()==Qt.MouseButton.LeftButton:
            self.clicks+=1
            if self.clicks==5:
                self.clicks=0;self.activated.emit()
        super().mouseReleaseEvent(event)


PROVERBS = {'ru': ('Терпенье и труд всё перетрут.',
        'Под лежачий камень вода не течёт.',
        'Дорогу осилит идущий.',
        'Без труда не вытащишь и рыбку из пруда.',
        'Вода камень точит.',
        'Тише едешь — дальше будешь.',
        'Семь раз отмерь, один раз отрежь.',
        'Конец — делу венец.',
        'Дело мастера боится.',
        'Не боги горшки обжигают.',
        'Глаза боятся, а руки делают.',
        'Лиха беда начало.',
        'Маленькое дело лучше большого безделья.',
        'Кто ищет, тот найдёт.',
        'Всё хорошо, что хорошо кончается.'),
 'en': ('Slow and steady wins the race.',
        'Where there’s a will, there’s a way.',
        'If at first you don’t succeed, try, try again.',
        'Practice makes perfect.',
        'Every cloud has a silver lining.',
        'All’s well that ends well.',
        'Better late than never.',
        'Rome wasn’t built in a day.',
        'Nothing ventured, nothing gained.',
        'Fortune favours the bold.',
        'The early bird catches the worm.',
        'Little strokes fell great oaks.',
        'Well begun is half done.',
        'Good things come to those who wait.',
        'When one door closes, another opens.'),
 'es': ('Quien la sigue, la consigue.',
        'Con paciencia todo se logra.',
        'La paciencia es la madre de la ciencia.',
        'El que busca halla.',
        'El que algo quiere, algo le cuesta.',
        'No se ganó Zamora en una hora.',
        'Más vale tarde que nunca.',
        'A quien madruga, Dios le ayuda.',
        'No hay mal que por bien no venga.',
        'Después de la tormenta viene la calma.',
        'Quien no arriesga, no gana.',
        'Obras son amores, y no buenas razones.',
        'Lo bien hecho, bien parece.',
        'Nunca es tarde si la dicha es buena.',
        'Las cosas de palacio van despacio.')}


class AppointmentChase(QDialog):
    def __init__(self,parent,translate,*,language="en"):
        super().__init__(parent)
        from PySide6.QtCore import QTimer,QSize,QByteArray
        from PySide6.QtGui import QIcon,QPixmap,QPainter
        from PySide6.QtSvg import QSvgRenderer
        import time
        self.language=language; self.particles=[]; self.frames=0
        self.t=translate;self.moves=0;self.caught=False;self.last_escape=time.monotonic()
        self.setWindowTitle(translate('Последняя сита'));self.setFixedSize(760,480)
        self.message=QLabel(translate('Поймать ситу'),self)
        self.message.setGeometry(20,16,720,32);self.message.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.congratulations=QLabel(translate('Сита поймана!'),self)
        self.congratulations.setGeometry(30,175,700,48);self.congratulations.setAlignment(Qt.AlignmentFlag.AlignCenter)
        font=self.congratulations.font();font.setPixelSize(30);font.setBold(True);self.congratulations.setFont(font);self.congratulations.hide()
        self.proverb=QLabel(self);self.proverb.setGeometry(60,270,640,80)
        self.proverb.setAlignment(Qt.AlignmentFlag.AlignCenter);self.proverb.setWordWrap(True);self.proverb.hide()
        self.celebration_timer=QTimer(self);self.celebration_timer.setInterval(30);self.celebration_timer.timeout.connect(self.animate_celebration)
        self.ticket=QPushButton(self);self.ticket.setAccessibleName(translate('Сита!'))
        self.ticket.setGeometry(350,220,56,56);self.ticket.setIconSize(QSize(48,48))
        self.ticket.setStyleSheet('QPushButton { background:transparent; border:0; padding:0; }')
        svg=b'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect x="4" y="8" width="40" height="36" rx="7" fill="#5E6AD2"/><path d="M14 5v9M34 5v9M11 20h26m-22 8h5m8 0h5m-18 7h5" fill="none" stroke="white" stroke-width="3" stroke-linecap="round"/></svg>'
        pixmap=QPixmap(96,96);pixmap.fill(Qt.GlobalColor.transparent);pixmap.setDevicePixelRatio(2)
        painter=QPainter(pixmap);QSvgRenderer(QByteArray(svg)).render(painter);painter.end()
        self.ticket.setIcon(QIcon(pixmap));self.ticket.clicked.connect(self.catch)
        self.timer=QTimer(self);self.timer.setInterval(30);self.timer.timeout.connect(self.watch_cursor);self.timer.start()
    def watch_cursor(self):
        import time
        from PySide6.QtGui import QCursor
        cursor=self.mapFromGlobal(QCursor.pos())
        center=self.ticket.geometry().center()
        if not self.caught and self.isActiveWindow() and time.monotonic()-self.last_escape>.18 and (center-cursor).manhattanLength()<110:
            self.evade(cursor)
    def evade(self,cursor):
        import random,time
        if self.caught:return
        candidates=[QPoint(random.randint(20,self.width()-76),random.randint(65,self.height()-76)) for _ in range(16)]
        destination=max(candidates,key=lambda p:(p+QPoint(28,28)-cursor).manhattanLength())
        self.ticket.move(destination);self.moves+=1;self.last_escape=time.monotonic()
    def catch(self):
        self.caught=True;self.timer.stop()
        import random
        self.message.setText(self.t('Сита поймана'));self.message.setGeometry(30,230,700,32)
        self.ticket.setEnabled(False);self.ticket.hide();self.congratulations.show()
        self.proverb.setText(random.choice(PROVERBS.get(self.language,PROVERBS['en'])));self.proverb.show()
        colors=('#5E6AD2','#E9B44C','#6BCF8E','#E4685D','#8A93F5')
        self.particles=[[random.uniform(0,self.width()),random.uniform(-220,0),random.uniform(-2,2),random.uniform(2,6),random.choice(colors),random.uniform(0,360)] for _ in range(100)]
        self.frames=0;self.celebration_timer.start();self.update()

    def animate_celebration(self):
        self.frames+=1
        for particle in self.particles:
            particle[0]+=particle[2];particle[1]+=particle[3];particle[5]+=4
        if self.frames>=100:
            self.celebration_timer.stop();self.particles=[]
        self.update()

    def paintEvent(self,event):
        super().paintEvent(event)
        from PySide6.QtGui import QPainter,QColor
        painter=QPainter(self);painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setPen(Qt.PenStyle.NoPen)
        for x,y,_,_,color,angle in self.particles:
            painter.save();painter.translate(x,y);painter.rotate(angle);painter.setBrush(QColor(color));painter.drawRoundedRect(-3,-5,6,10,1,1);painter.restore()
        painter.end()
    def closeEvent(self,event):
        self.timer.stop();self.celebration_timer.stop();super().closeEvent(event)
