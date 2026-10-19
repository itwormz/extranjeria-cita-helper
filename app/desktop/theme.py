"""Shared semantic tokens for native UI, status and activity events."""
from pathlib import Path
from PySide6.QtGui import QColor, QFont, QFontDatabase, QPalette


def theme_colors(dark):
    return (dict(bg='#1D1E22', surface='#26272C', text='#E8E8E8', muted='#9A9B9F',
        line='#3A3B40', line_strong='#767780', accent='#5E6AD2', accent_text='#8A93F5',
        attention='#E9B44C', danger='#E4685D', success='#6BCF8E', selection='#33354D',
        attention_soft='#38301C', danger_soft='#3A2422', bitcoin='#F7931A', on_brand='#FFFFFF') if dark else
        dict(bg='#FFFFFF', surface='#F7F6F5', text='#282A30', muted='#6F6F6F',
        line='#E6E6E6', line_strong='#8A8A8A', accent='#5E6AD2', accent_text='#5E6AD2',
        attention='#8A5A00', danger='#B03A2E', success='#1B7A5A', selection='#E6E8F6',
        attention_soft='#FFF0D0', danger_soft='#FBE4E0', bitcoin='#F7931A', on_brand='#FFFFFF'))


def load_fonts(app):
    if app.property('plexFontsLoaded'): return
    app.setProperty('plexFontsLoaded',True)
    for path in (Path(__file__).parent/'assets'/'fonts').glob('*.ttf'):
        QFontDatabase.addApplicationFont(str(path))
    app.setFont(QFont('IBM Plex Sans', 10))


def apply_theme(app, *, dark):
    c = theme_colors(dark)
    palette = QPalette()
    roles = {'Window':'bg', 'WindowText':'text', 'Base':'surface', 'AlternateBase':'surface',
        'Text':'text', 'Button':'surface', 'ButtonText':'text', 'Highlight':'selection',
        'HighlightedText':'text', 'ToolTipBase':'surface', 'ToolTipText':'text', 'Link':'accent'}
    for role, key in roles.items():
        palette.setColor(getattr(QPalette.ColorRole, role), QColor(c[key]))
    palette.setColor(QPalette.ColorRole.Mid, QColor(c['line_strong']))
    for role in ('Text','ButtonText','WindowText'):
        palette.setColor(QPalette.ColorGroup.Disabled,getattr(QPalette.ColorRole,role),QColor(c['muted']))
    app.setPalette(palette)
    assets=Path(__file__).parent/'assets'
    arrow_down=(assets/('chevron-down-dark.svg' if dark else 'chevron-down-light.svg')).as_posix()
    arrow_up=(assets/('chevron-up-dark.svg' if dark else 'chevron-up-light.svg')).as_posix()
    app.setStyleSheet(f'''
        QWidget {{ font-family: "IBM Plex Sans"; font-size: 13px; }}
        QMainWindow, QDialog, QScrollArea, QScrollArea > QWidget > QWidget {{ background: {c['bg']}; }}
        QLabel#heading, QLabel#pageHeading {{ font-size:22px; font-weight:600; }}
        QLabel#sectionHeading {{ font-size:16px; font-weight:600; }}
        QLabel#timer {{ font-family:"IBM Plex Mono"; font-size:32px; font-weight:500; }}
        QLabel#metrics {{ font-family:"IBM Plex Mono"; font-size:12px; }}
        QLabel#stateLabel, QLabel#nextAction {{ font-weight:600; }}
        QLabel[muted="true"] {{ color:{c['muted']}; font-size:12px; }}
        QLabel#statisticsSection {{ font-size:14px; font-weight:600; }}
        QLabel#overviewValue {{ font-size:26px; font-weight:600; }}
        QLabel#appointmentValue {{ font-size:13px; font-weight:500; }}
        QFrame#statisticsOverview {{ background:{c['surface']}; border:1px solid {c['line']}; border-radius:7px; }}
        QFrame#socialDivider {{ background:{c['line_strong']}; }}
        QPushButton#donationPayment {{ min-height:0; padding:0; }}
        QLabel#statisticsTitle {{ font-size:22px; font-weight:600; }}
        QLabel#achievementDuration {{ font-size:21px; font-weight:500; }}
        QLabel#statisticValue {{ font-size:15px; font-weight:500; }}
        QPushButton[shareMode="true"] {{ padding:6px 12px; }}
        QPushButton[shareMode="true"]:checked {{ background:{c['accent']}; color:#FFFFFF; border-color:{c['accent']}; }}
        QFrame#aboutUpdates {{ background:{c['surface']}; border:1px solid {c['line']}; border-radius:7px; }}
        QFrame#calendarPreview {{ background:{c['surface']}; border:1px solid {c['line']}; border-radius:7px; }}
        QLabel#calendarResultTitle {{ font-weight:600; }}
        QFrame#shareResult {{ background:{c['surface']}; border:1px solid {c['line']}; border-radius:7px; }}
        QLabel#shareResultTitle {{ font-weight:600; color:{c['success']}; }}
        QTextEdit#shareText {{ background:{c['surface']}; color:{c['text']}; border:1px solid {c['line_strong']}; border-radius:7px; padding:12px; }}
        QTextEdit#shareText:focus {{ border:1px solid {c['accent']}; }}
        QFrame#achievementCard {{ background:{c['surface']}; border:1px solid {c['line']}; border-top:4px solid {c['success']}; border-radius:8px; }}
        QFrame#statisticRow {{ border:0; border-top:1px solid {c['line']}; }}
        QFrame#statisticsSeparator {{ background:{c['line']}; }}
        QToolButton#social_telegram {{ background:#229ED9; border:1px solid #229ED9; color:white; }}
        QToolButton#social_x {{ background:#121212; border:1px solid {c['line_strong']}; color:white; }}
        QToolButton#social_facebook {{ background:#1877F2; border:1px solid #1877F2; color:white; }}
        QToolButton#social_instagram {{ background:qlineargradient(x1:0,y1:0,x2:1,y2:1,stop:0 #7438BB,stop:0.5 #BF2B78,stop:1 #D95539); border:1px solid #B34787; color:white; }}
        QWidget#stateCard {{ background:{c['surface']}; border:1px solid {c['line']}; border-radius:8px; }}
        QWidget#stateRail {{ background:{c['muted']}; }}
        QWidget#stateRail[tone="accent"] {{ background:{c['accent']}; }}
        QLabel#stateLabel[tone="accent"] {{ color:{c['accent_text']}; }}
        QWidget#stateRail[tone="attention"] {{ background:{c['attention']}; }}
        QLabel#stateLabel[tone="attention"], QLabel#participationLabel[tone="attention"] {{ color:{c['attention']}; }}
        QWidget#stateRail[tone="danger"] {{ background:{c['danger']}; }}
        QLabel#stateLabel[tone="danger"] {{ color:{c['danger']}; }}
        QWidget#errorBand {{ background:{c['danger_soft']}; border-left:3px solid {c['danger']}; border-radius:6px; }}
        QLabel[reportTitle="true"] {{ font-size:16px; font-weight:600; }}
        QLabel#formError {{ color:{c['danger']}; }}
        QPushButton {{ min-height:18px; padding:8px 16px; border:1px solid {c['line_strong']}; border-radius:6px; background:{c['surface']}; color:{c['text']}; }}
        QPushButton:hover, QToolButton:hover {{ border-color:{c['accent']}; }}
        QPushButton[primary="true"] {{ background:{c['accent']}; color:#FFFFFF; border-color:{c['accent']}; }}
        QPushButton[reportCopied="true"] {{ background:{c['surface']}; color:{c['success']}; border-color:{c['success']}; }}
        QPushButton:disabled, QPushButton[primary="true"]:disabled {{ color:{c['muted']}; background:{c['bg']}; border:1px solid {c['line']}; }}
        QPushButton:focus, QToolButton:focus, QLineEdit:focus, QComboBox:focus, QDoubleSpinBox:focus {{ border:2px solid {c['accent']}; }}
        QToolButton {{ color:{c['text']}; padding:0; border:1px solid {c['line_strong']}; border-radius:6px; background:{c['surface']}; }}
        QToolButton:disabled {{ color:{c['muted']}; border-color:{c['line']}; background:{c['bg']}; }}
        QToolButton[toolbarAction="true"], QPushButton[toolbarAction="true"] {{ border:1px solid transparent; background:transparent; }}
        QToolButton[toolbarAction="true"]:hover, QPushButton[toolbarAction="true"]:hover {{ background:{c['surface']}; border-color:transparent; }}
        QToolButton[toolbarAction="true"]:focus, QPushButton[toolbarAction="true"]:focus {{ border:1px solid {c['accent']}; }}
        QToolButton::menu-indicator {{ image:none; }}
        QMenu {{ background:{c['surface']}; color:{c['text']}; border:1px solid {c['line']}; padding:5px; }}
        QMenu::item {{ padding:8px 18px; }}
        QMenu::item:selected {{ background:{c['selection']}; }}
        QLineEdit, QComboBox, QDoubleSpinBox, QDateEdit, QTimeEdit {{ padding:7px 10px; min-height:19px; border:1px solid {c['line_strong']}; border-radius:6px; background:{c['surface']}; color:{c['text']}; }}
        QFrame#comboPopup {{ border:0; padding:0; background:{c['surface']}; }}
        QComboBox::drop-down {{ width:28px; border:0; background:transparent; }}
        QComboBox::down-arrow {{ image:url("{arrow_down}"); width:14px; height:14px; }}
        QComboBox QAbstractItemView {{ background:{c['surface']}; color:{c['text']}; border:1px solid {c['line_strong']}; padding:4px; outline:0; selection-background-color:{c['selection']}; selection-color:{c['text']}; }}
        QComboBox QAbstractItemView::item {{ min-height:28px; padding:4px 10px; border:0; border-radius:4px; }}
        QComboBox QAbstractItemView::item:selected {{ background:{c['selection']}; color:{c['text']}; }}
        QComboBox QAbstractItemView::item:hover {{ background:{c['selection']}; }}
        QDateEdit::up-button, QDateEdit::down-button, QTimeEdit::up-button, QTimeEdit::down-button {{ subcontrol-origin:border; subcontrol-position:top right; width:0; height:0; border:0; }}
        QWidget#profileField QDateEdit, QWidget#profileField QTimeEdit {{ background:transparent; border:0; padding:0; min-height:0; }}
        QDateEdit QLineEdit, QTimeEdit QLineEdit {{ border:0; padding:0; min-height:0; background:transparent; }}
        QDoubleSpinBox::up-button, QDoubleSpinBox::down-button {{ subcontrol-origin:border; subcontrol-position:top right; width:0; height:0; border:0; }}
        QDoubleSpinBox::up-arrow, QDoubleSpinBox::down-arrow {{ image:none; width:0; height:0; }}
        QDoubleSpinBox QLineEdit, QDoubleSpinBox QLineEdit:focus {{ border:0; border-radius:0; padding:0; min-height:0; background:transparent; }}
        QFrame#appointmentField {{ background:{c['surface']}; border:1px solid {c['line_strong']}; border-radius:7px; }}
        QFrame#appointmentField[focused="true"] {{ border:1px solid {c['accent']}; }}
        QFrame#appointmentField[invalid="true"] {{ border:1px solid {c['danger']}; }}
        QFrame#appointmentField QLineEdit, QFrame#appointmentField QLineEdit:focus, QFrame#appointmentField QComboBox, QFrame#appointmentField QComboBox:focus {{ border:0; background:transparent; padding:0; min-height:24px; selection-background-color:{c['accent']}; selection-color:#FFFFFF; }}
        QToolButton#appointmentCalendarButton {{ border:0; background:transparent; }}
        QLabel#appointmentError {{ color:{c['danger']}; font-size:12px; }}
        QFrame#appointmentCalendarPopup {{ background:{c['surface']}; border:1px solid {c['line_strong']}; border-radius:8px; }}
        QCalendarWidget#appointmentCalendar {{ background:{c['surface']}; color:{c['text']}; }}
        QCalendarWidget#appointmentCalendar QWidget#qt_calendar_navigationbar {{ background:{c['surface']}; }}
        QCalendarWidget#appointmentCalendar QToolButton {{ color:{c['text']}; border:0; padding:4px; background:{c['surface']}; }}
        QCalendarWidget#appointmentCalendar QSpinBox {{ background:{c['surface']}; color:{c['text']}; border:1px solid {c['accent']}; border-radius:4px; padding:2px; }}
        QCalendarWidget#appointmentCalendar QAbstractItemView {{ outline:0; background:{c['surface']}; color:{c['text']}; selection-background-color:{c['accent']}; selection-color:#FFFFFF; border:0; }}
        QWidget#profileField, QWidget#timingField {{ background:{c['surface']}; border:1px solid {c['line_strong']}; border-radius:7px; }}
        QWidget#profileField[focused="true"] {{ border-color:{c['accent']}; }}
        QWidget#timingField QLineEdit, QWidget#timingField QDoubleSpinBox, QWidget#profileField QLineEdit, QWidget#profileField QComboBox {{ background:transparent; border:0; border-radius:0; padding:0; min-height:0; }}
        QWidget#profileField QLineEdit:focus, QWidget#profileField QComboBox:focus {{ border:0; }}
        QLabel#compactTimer {{ font-family:"IBM Plex Mono"; font-size:20px; }}
        QLabel#compactMetrics {{ font-size:11px; }}
        QLabel#profileCaption {{ color:{c['muted']}; font-size:11px; }}
        QPushButton[profileMode="true"] {{ padding:5px 9px; font-size:12px; }}
        QPushButton[profileMode="true"]:checked {{ background:{c['selection']}; border-color:{c['accent']}; }}
        QWidget#settingRow {{ background:{c['surface']}; border:1px solid {c['line']}; border-radius:8px; }}
        QTabWidget::pane {{ border:0; border-top:1px solid {c['line']}; }}
        QTabBar::tab {{ padding:10px 16px; border-bottom:2px solid transparent; color:{c['muted']}; }}
        QTabBar::tab:selected {{ color:{c['text']}; border-bottom-color:{c['accent']}; }}
        QTableWidget {{ outline:0; border:1px solid {c['line']}; background:{c['surface']}; }}
        QHeaderView::section {{ padding:8px; border:0; border-bottom:1px solid {c['line']}; background:{c['surface']}; color:{c['muted']}; }}
        QListWidget {{ outline:0; background:{c['surface']}; border:1px solid {c['line']}; border-radius:6px; padding:7px; }}
        QListWidget::item {{ padding:9px 8px; }}
        QListWidget::item:selected {{ background:{c['selection']}; color:{c['text']}; }}
        QToolTip {{ color:{c['text']}; background:{c['surface']}; border:1px solid {c['line']}; padding:7px; }}
        QScrollBar:vertical {{ background:{c['bg']}; width:10px; margin:0; }}
        QScrollBar::handle:vertical {{ background:{c['line_strong']}; min-height:24px; border-radius:4px; }}
        QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {{ height:0; }}
        QScrollBar::add-page:vertical, QScrollBar::sub-page:vertical {{ background:none; }}
        QScrollBar:horizontal {{ background:{c['bg']}; height:10px; margin:0; }}
        QScrollBar::handle:horizontal {{ background:{c['line_strong']}; min-width:24px; border-radius:4px; }}
        QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal {{ width:0; }}
        QScrollBar::add-page:horizontal, QScrollBar::sub-page:horizontal {{ background:none; }}
        QWidget#monitorPage QLabel, QWidget#monitorPage QPushButton, QWidget#monitorPage QComboBox {{ font-size:14px; }}
        QWidget#monitorPage QLabel[muted="true"] {{ font-size:14px; }}
        QWidget#monitorPage QLabel#stateLabel, QWidget#monitorPage QLabel#nextAction {{ font-weight:500; }}
        QWidget#monitorPage QLabel#compactMetrics {{ font-family:"IBM Plex Sans"; font-size:14px; }}
        QWidget#monitorPage QLabel#compactTimer {{ font-family:"IBM Plex Mono"; font-size:24px; font-weight:500; }}
        QFrame#journalSeparator {{ background:{c['line']}; border:0; }}
        QTableWidget#journalTable::item {{ padding:0 8px; }}
        QWidget#timerColumn {{ border-left:1px solid {c['line']}; }}
        QTabBar#mainNavigation::tab {{ font-size:14px; padding:14px 20px; border:0; border-bottom:2px solid transparent; background:transparent; }}
        QTabBar#mainNavigation::tab:selected {{ color:{c['accent']}; border-bottom:2px solid {c['accent']}; }}
        QTabWidget#journalTabs {{ border:0; }}
        QTabWidget#journalTabs::pane {{ border:0; }}
        QTabWidget#journalTabs QTabBar::tab {{ font-size:14px; padding:12px 10px; }}
        QTableWidget#journalTable {{ font-size:14px; border:0; background:{c['bg']}; }}
        QTableWidget#journalTable QHeaderView::section {{ font-size:14px; padding:10px 8px; background:{c['bg']}; }}
        QWidget#statisticsPanel QLabel, QWidget#statisticsPanel QPushButton, QWidget#statisticsPanel QComboBox {{ font-size:14px; }}
        QWidget#statisticsPanel QLabel[muted="true"] {{ font-size:14px; }}
        QWidget#statisticsPanel QLabel#statisticsTitle {{ font-size:20px; font-weight:500; }}
        QWidget#statisticsPanel QLabel#overviewValue {{ font-size:26px; font-weight:500; }}
        QWidget#statisticsPanel QLabel#statisticsSection, QWidget#statisticsPanel QLabel#statisticValue {{ font-size:14px; font-weight:500; }}
        QSplitter::handle {{ background:{c['line']}; height:1px; }}
    ''')
    return c
