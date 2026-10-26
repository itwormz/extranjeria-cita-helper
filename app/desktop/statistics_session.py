"""Connect process-owned controls to independent, local statistics."""
import logging
import platform
import uuid

from ..version import VERSION

log = logging.getLogger(__name__)


class StatisticsSession:
    def __init__(self, statistics, task, control):
        self.statistics, self.profile_id, self.control = statistics, task.id, control
        self.session_id = uuid.uuid4().hex
        self.closed = False
        self._pass_seen = False
        self._error_kind = None
        control.set_history_sink(self.record)
        try:
            statistics.start_session(task.id, self.session_id, {
                'province': task.province, 'procedure': task.procedure,
                'authentication': task.access_mode,
                'clave_method': getattr(task, 'clave_method', ''),
                'no_slots_mode':getattr(task,'no_slots_mode','reload'),
                'app_version': VERSION, 'os': platform.system(),
                'architecture': platform.machine(),
            })
            statistics.event(task.id,'effort_started',session_id=self.session_id,counted=False)
            control.set_check_counter(statistics.total(task.id), self.increment)
            control.set_statistics_sink(self.observe, self.add_active, add_active_at=self.add_active_at)
        except Exception:
            self.failed()

    def failed(self):
        self.control._state['statistics_error'] = True
        log.warning('event=statistics_write_failed')

    def increment(self):
        return self.statistics.increment(self.profile_id, session_id=self.session_id)

    def observe(self, kind, document, **fields):
        if kind in {'server_error','rate_limited','blocked','expired'}:
            counted = self._error_kind != kind
            self._error_kind = kind
            return self.statistics.event(self.profile_id,kind,document,
                session_id=self.session_id,counted=counted,**fields)
        if kind == 'pass' and not self._pass_seen:
            self._pass_seen = True
            return self.statistics.event(self.profile_id,kind,document,
                session_id=self.session_id,counted=False,**fields)
        return self.statistics.observe(self.profile_id, kind, document,
                                       session_id=self.session_id, summary=False, **fields)

    def add_active(self, seconds):
        self.statistics.add_active(self.profile_id, seconds, session_id=self.session_id, summary=False)

    def add_active_at(self, seconds, at):
        self.statistics.add_active(self.profile_id, seconds, at=at, session_id=self.session_id, summary=False)

    def record(self, kind, **fields):
        store, profile, session = self.statistics, self.profile_id, self.session_id
        if kind == 'page':
            if fields.get('stage') not in {'server_error','rate_limited','blocked','expired','loading','unknown'}:
                self._error_kind = None
            return store.observe_page(profile, session, **fields)
        if kind in {'phase', 'gap'}:
            if kind == 'gap':self._error_kind = None
            return store.phase(profile, session, fields['mode'], at=fields.get('at'))
        if kind == 'heartbeat':
            at = fields.get('at')
            if fields.get('mode'):
                store.phase(profile, session, fields['mode'], at=at)
            return store.heartbeat(profile, session, at=at)
        # Presentation text/URLs never cross this boundary.
        return store.event(profile, kind, session_id=session, counted=False, **fields)

    def close(self, reason):
        if self.closed:
            return
        self.closed = True
        self.control.flush_statistics()
        try:
            self.statistics.end_session(self.profile_id, self.session_id, reason)
        except Exception:
            self.failed()
        self.control.set_history_sink(None)
