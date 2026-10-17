"""Application icon with a small, static status indicator."""
from pathlib import Path
from PySide6.QtCore import Qt, QRectF
from PySide6.QtGui import QColor, QIcon, QPainter, QPen, QPixmap
from PySide6.QtSvg import QSvgRenderer


def icon_status(state, active, worker_error=False):
    if worker_error or state.get('failed') or state.get('connection_lost'):
        return 'error'
    if state.get('booking_handoff') or state.get('browser_retained') or state.get('mode') == 'manual':
        return 'attention'
    if state.get('paused') or state.get('mode') == 'paused':
        return 'paused' if active else 'idle'
    return 'running' if active and state.get('mode') != 'finished' else 'idle'


def application_icon(status='idle'):
    icon=QIcon()
    renderer=QSvgRenderer(str(Path(__file__).parent/'assets'/'icon.svg'))
    colors={'running':'#37bb78', 'paused':'#8b8e99', 'attention':'#edb548', 'error':'#e56568'}
    for size in (16,32,48,64,128,256,512):
        pixmap=QPixmap(size,size);pixmap.fill(Qt.GlobalColor.transparent)
        painter=QPainter(pixmap);painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        renderer.render(painter)
        if status in colors:
            # Neutral double outline keeps the dot visible on any desktop.
            dot=QRectF(size*.735,size*.735,size*.225,size*.225)
            painter.setBrush(QColor(colors[status]))
            painter.setPen(QPen(QColor('#93959e'),max(1,size*.035)))
            painter.drawEllipse(dot)
            painter.setPen(QPen(QColor('#ffffff'),max(.75,size*.02)))
            painter.drawEllipse(dot)
        painter.end();icon.addPixmap(pixmap)
    return icon
