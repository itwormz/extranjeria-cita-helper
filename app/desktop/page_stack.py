"""Only the visible page contributes to stacked-view layout sizes."""
from PySide6.QtCore import Signal
from PySide6.QtWidgets import QWidget,QVBoxLayout


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
    def setCurrentIndex(self,index):self.setCurrentWidget(self.pages[index])
    def count(self):return len(self.pages)
    def widget(self,index):return self.pages[index]

    def setCurrentWidget(self,page):
        index=self.pages.index(page)
        if index==self.current:return
        if self.current>=0:self.pages[self.current].hide()
        self.current=index;page.show();self.layout().invalidate();self.updateGeometry();self.currentChanged.emit(index)
