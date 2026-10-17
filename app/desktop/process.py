"""Non-blocking Qt adapter for the single browser worker."""
import time

from app.browser_visibility import allow_worker_foreground

from PySide6.QtCore import QObject, QProcess, QProcessEnvironment, QTimer, Signal
from .protocol import JsonLinesDecoder, encode_message
from .paths import runtime_root, worker_arguments, worker_executable


class DesktopController(QObject):
    state_changed = Signal(dict)
    failed = Signal(str)
    finished = Signal(int)

    def __init__(self, store):
        super().__init__()
        self.store = store
        self.profile_id = None
        self.process = QProcess(self)
        self.process.setWorkingDirectory(str(runtime_root()))
        self.process.readyReadStandardOutput.connect(self._read)
        self.process.readyReadStandardError.connect(lambda: self.process.readAllStandardError())
        self.process.finished.connect(self._finished)
        self.process.errorOccurred.connect(self._error)
        self.decoder = JsonLinesDecoder(limit=2 * 1024 * 1024)
        self.last_state = {}
        self.last_message = 0
        self.stopping = False
        self.closing_session = False
        self.timer = QTimer(self)
        self.timer.setInterval(1000)
        self.timer.timeout.connect(self.check_heartbeat)
        self.terminate_timer = QTimer(self); self.terminate_timer.setSingleShot(True)
        self.terminate_timer.timeout.connect(self._terminate)
        self.kill_timer = QTimer(self); self.kill_timer.setSingleShot(True)
        self.kill_timer.timeout.connect(self._kill)

    @property
    def active(self):
        return self.process.state() != QProcess.ProcessState.NotRunning

    def start(self, profile_id):
        if self.active:
            return
        self.profile_id = profile_id
        env = QProcessEnvironment.systemEnvironment()
        env.insert('TASKS_FILE', str(self.store.path))
        env.insert('CATALOG_FILE', str(self.store.catalog_path))
        env.insert('PYTHONUNBUFFERED', '1')
        self.process.setProcessEnvironment(env)
        self.decoder = JsonLinesDecoder(limit=2 * 1024 * 1024)
        self.stopping = False
        self.closing_session = False
        self.last_state = dict(screen='Запуск Camoufox', evidence='', next_action='Открыть сайт', mode='working')
        self.last_message = time.monotonic()
        self.process.start(str(worker_executable()), worker_arguments(profile_id))
        self.timer.start()
        self.state_changed.emit(self.last_state)

    def send(self, command):
        if self.active:
            if command.get('command') == 'show_browser':
                # The panel received the click; the background worker did not.
                # Grant only this QProcess, never ASFW_ANY or another session.
                allow_worker_foreground(int(self.process.processId()))
            self.process.write(encode_message(command))

    def _read(self):
        for message in self.decoder.feed(bytes(self.process.readAllStandardOutput())):
            if message.get('type') in {'ready', 'state'}:
                self.last_message = time.monotonic()
            if message.get('type') == 'state' and isinstance(message.get('state'), dict):
                self.last_state = message['state']
                if self.last_state.get('browser_retained') and not self.closing_session:
                    self.terminate_timer.stop()
                    self.kill_timer.stop()
                self.state_changed.emit(self.last_state)
            elif message.get('type') == 'error':
                self.failed.emit(message.get('message', 'Ошибка процесса бота'))
            elif message.get('type') == 'protocol_error':
                self.failed.emit('Не удалось прочитать сообщение бота — ждём следующее обновление.')

    def check_heartbeat(self):
        if self.active and time.monotonic() - self.last_message > 3:
            self.state_changed.emit(dict(self.last_state, connection_lost=True))

    def shutdown(self, *, close_browser=False):
        if close_browser:
            self.closing_session = True
        if self.active and (not self.stopping or close_browser):
            self.stopping = True
            self.send({'command':'shutdown' if close_browser else 'stop'})
            self.terminate_timer.start(10_000)

    def _terminate(self):
        if self.active:
            self.process.terminate()
            self.kill_timer.start(3_000)

    def _kill(self):
        if self.active:
            self.process.kill()

    def _finished(self, code, status):
        self._read()
        for timer in (self.timer, self.terminate_timer, self.kill_timer):
            timer.stop()
        self.finished.emit(code)

    def _error(self, error):
        if error == QProcess.ProcessError.FailedToStart:
            self.timer.stop()
            self.failed.emit('Не удалось запустить процесс бота.')
            self.finished.emit(1)
