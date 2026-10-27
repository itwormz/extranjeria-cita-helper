"""Process-owned assist state; the site and UI never own the pause flag."""
import json
import logging
import os
from datetime import datetime
import threading
import time

from .assist_journal import PageJournal


class AssistControl:
    def __init__(self):
        self._lock = threading.RLock()
        self._paused = False
        self._timeline_path = None
        self._timeline = []
        self._availability = []
        self._journal = PageJournal(self._persist_journal)
        self.stop_requested = False
        self.close_browser_requested = False
        self.browser_retained = False
        self.booking_handoff = False
        self.restart_requested = False
        self._history_manual_restart = False
        self.diagnostics_enabled = None
        self.sound_enabled = True
        self.show_browser_requested = False
        self.blocked_streak = 0
        self._check_counter = None
        self._statistics_observer = None
        self._statistics_active = None
        self._statistics_active_at = None
        self._statistics_pending_at = None
        self._statistics_retry = []
        self._statistics_tick_at = None
        self._statistics_wall_at = None
        self._statistics_was_active = False
        self._statistics_pending_seconds = 0
        self._history_sink = None
        self._history_phase = None
        self._history_stage = ""
        self._history_pulse_at = None
        self._state = dict(screen="Запуск", evidence="", next_action="Открыть браузер",
                           mode="working", remaining_seconds=None, requests=0, checks=0, profile_checks=0,
                           updated_at=time.time(), process_at=time.time(), finished=False, statistics_error=False)
        self._remaining_at = time.monotonic()

    def set_timeline_path(self, path):
        with self._lock:
            self._timeline_path = path

    def _persist_journal(self, row):
        if not self._timeline_path:
            return
        try:
            path = self._timeline_path.with_name('journal.jsonl')
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
            with os.fdopen(fd, 'a', encoding='utf-8') as stream:
                stream.write(json.dumps(row, ensure_ascii=False) + '\n')
        except OSError:
            logging.getLogger(__name__).warning('event=journal_write_failed')

    def observe_visit(self, *, document, stage, page, title, recognition="known", evidence=(), variant="", automated=True):
        with self._lock:
            if automated and document and stage not in {'loading','unknown'} and not self._paused and not self.booking_handoff:
                self.observe_statistics('processed_page',document)
            self._journal.observe(document=document, stage=stage, page=page, title=title)
            self._history_stage = stage
            self._history("page", stage=stage, document=document, recognition=recognition, evidence=evidence, variant=variant)

    def update_offices(self, offices, selected=None):
        with self._lock:
            self._journal.offices(offices, selected)

    def set_session(self, run_id):
        with self._lock:
            self._state["run_id"] = run_id
            self.booking_handoff = False

    def request_stop(self):
        self.flush_statistics()
        self.stop_requested = True

    def request_restart(self):
        with self._lock:
            self.restart_requested = True
            self._history_manual_restart = True
            if self.booking_handoff:
                # Explicit restart ends a prior pause too, but the handoff
                # stays latched until the old browser context is closed.
                self._paused = False

    def clear_restart(self):
        self.restart_requested = False

    def set_diagnostics(self, enabled):
        self.diagnostics_enabled = bool(enabled)

    def set_sound(self, enabled):
        self.sound_enabled = bool(enabled)

    def record(self, event, *, page="", stage="", detail="", document=None):
        with self._lock:
            if not page and self._timeline:
                page = self._timeline[-1]["page"]
            row = dict(time=datetime.now().astimezone().isoformat(timespec="milliseconds"),
                       event=event, page=page, stage=stage, detail=detail)
            self._journal.event(event, detail=detail, document=document, stage=stage, page=page,
                                user_requested=self.stop_requested or self.close_browser_requested)
            if event not in {'screen', 'capture', 'response', 'check', 'offices_seen', 'booking_seen', 'statistics_error'}:
                fields = dict(stage=stage if stage else self._history_stage, document=document or '')
                if event == 'flow_restart':
                    fields.update(previous_stage=self._history_stage, initiator='user' if self._history_manual_restart else 'automatic')
                    self._history_manual_restart = False
                self._history(event, **fields)
            self._timeline.append(row)
            self._timeline = self._timeline[-200:]
            if self._timeline_path:
                try:
                    fd = os.open(self._timeline_path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
                    with os.fdopen(fd, "a", encoding="utf-8") as stream:
                        stream.write(json.dumps(row, ensure_ascii=False) + "\n")
                except OSError:
                    logging.getLogger(__name__).warning("event=timeline_write_failed")
            return row

    def record_availability(self, event, *, page, offices, selected_office=None, timestamp=None):
        if event != "offices_seen":
            return None
        with self._lock:
            row = dict(time=timestamp or datetime.now().astimezone().isoformat(timespec="milliseconds"),
                       event=event, page=page,
                       offices=[dict(value=o["value"], text=o["text"]) for o in offices])
            self._availability.append(row)
            self._availability = self._availability[-200:]
            if self._timeline_path:
                try:
                    path = self._timeline_path.with_name("availability.jsonl")
                    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
                    with os.fdopen(fd, "a", encoding="utf-8") as stream:
                        stream.write(json.dumps(row, ensure_ascii=False) + "\n")
                except OSError:
                    logging.getLogger(__name__).warning("event=availability_write_failed")
            return row

    @property
    def paused(self):
        with self._lock:
            return self._paused

    def set_paused(self, paused):
        with self._lock:
            self.flush_statistics()
            remaining = self.snapshot()["remaining_seconds"]
            self._state["remaining_seconds"] = remaining
            self._remaining_at = time.monotonic()
            self._paused = bool(paused)
            self._statistics_was_active = False
            self.record("pause" if self._paused else "resume", stage=self._state["screen"], detail=self._state["next_action"])
            self._history_mode("paused" if self._paused else self._state["mode"])

    def publish(self, *, screen, evidence, next_action, mode, remaining_seconds=None):
        with self._lock:
            if mode in {'manual', 'paused', 'finished', 'booking_handoff'}:
                self.flush_statistics()
            self._state.update(screen=screen, evidence=evidence, next_action=next_action,
                               mode=mode, remaining_seconds=remaining_seconds,
                               updated_at=time.time())
            self._remaining_at = time.monotonic()
            if mode in {'manual', 'paused', 'finished', 'booking_handoff'}:
                self._statistics_was_active = False
            self._journal.phase(mode, remaining_seconds, next_action)
            self._history_mode("paused" if self._paused else mode)

    def touch(self):
        with self._lock:
            self._state["process_at"] = time.time()

    def set_history_sink(self, sink):
        """Typed persistent events; deliberately separate from page diagnostics."""
        with self._lock:
            self._history_sink = sink

    def _history(self, kind, **fields):
        if self._history_sink:
            try:
                self._history_sink(kind, **fields)
            except Exception:
                self._state['statistics_error'] = True
                logging.getLogger(__name__).warning('event=statistics_write_failed')

    def _history_mode(self, mode):
        if mode != self._history_phase:
            self._history('phase', mode=mode)
            self._history_phase = mode

    def browser_session_started(self):
        with self._lock:
            generation = getattr(self, '_browser_generation', 0) + 1
            self._browser_generation = generation
            self._history('browser_session_started', browser_generation=generation, previous_stage=self._history_stage)

    def response_observed(self, status):
        if type(status) is int and 100 <= status <= 599:
            with self._lock:
                self._history('http_response', stage=self._history_stage, http_status=status)

    def set_statistics_sink(self, observe, add_active, *, add_active_at=None):
        with self._lock:
            self._statistics_observer = observe
            self._statistics_active = add_active
            self._statistics_active_at = add_active_at

    def automation_action(self):
        """One completed semantic operation, with no selector or field data."""
        import uuid
        with self._lock:
            if not self._paused and not self.booking_handoff and self._state['mode']!='manual':
                self.observe_statistics('automation_action',uuid.uuid4().hex)

    def observe_statistics(self, kind, document, *, offices=()):
        with self._lock:
            if self._statistics_observer:
                try:
                    self._statistics_observer(kind, document, offices=offices)
                except Exception:
                    self._state['statistics_error'] = True
                    logging.getLogger(__name__).warning('event=statistics_write_failed')

    def flush_statistics(self):
        with self._lock:
            if not self._statistics_active:
                return
            pending = list(self._statistics_retry)
            self._statistics_retry = []
            if self._statistics_pending_seconds:
                pending.append((self._statistics_pending_seconds, self._statistics_pending_at))
                self._statistics_pending_seconds = 0
            for index,(seconds,at) in enumerate(pending):
                try:
                    if self._statistics_active_at:
                        self._statistics_active_at(seconds, at)
                    else:
                        self._statistics_active(seconds)
                except Exception:
                    self._state['statistics_error'] = True
                    # Keep timestamps across pause/sleep; never merge failed time
                    # into a later day. Bound memory during a prolonged disk fault.
                    self._statistics_retry = pending[index:][-120:]
                    logging.getLogger(__name__).warning('event=statistics_write_failed')
                    break

    def statistics_tick(self, now=None, *, wall_at=None):
        with self._lock:
            now = time.monotonic() if now is None else now
            wall_at = wall_at or datetime.now().astimezone().isoformat()
            active = (not self._paused and not self.stop_requested
                      and not self.close_browser_requested and not self.browser_retained
                      and not self.booking_handoff and not self._state['finished']
                      and self._state['mode'] not in {'manual', 'paused', 'finished', 'booking_handoff'})
            elapsed = 0 if self._statistics_tick_at is None else now - self._statistics_tick_at
            wall_elapsed = 0 if self._statistics_wall_at is None else (
                datetime.fromisoformat(wall_at.replace('Z', '+00:00')) -
                datetime.fromisoformat(self._statistics_wall_at.replace('Z', '+00:00'))).total_seconds()
            # macOS monotonic time can stop during system sleep. Offset-aware
            # wall timestamps also detect that missing time across DST changes.
            if elapsed > 2 or wall_elapsed > 2:
                self.flush_statistics()
            observation_gap = elapsed > 35 or wall_elapsed > 35
            if observation_gap:
                # The missing interval starts at the last worker tick, not
                # when execution resumes and notices that time was lost.
                self._history('gap', mode='unobserved', at=self._statistics_wall_at)
                self._history_phase = 'unobserved'
            if observation_gap or self._history_pulse_at is None or now - self._history_pulse_at >= 30:
                self._history('heartbeat', mode='paused' if self._paused else self._state['mode'], at=wall_at)
                self._history_pulse_at = now
            if active and self._statistics_was_active and 0 < elapsed <= 2 and 0 <= wall_elapsed <= 2 and self._statistics_active:
                self._statistics_pending_seconds += elapsed
                self._statistics_pending_at = wall_at or datetime.now().astimezone().isoformat()
                if self._statistics_pending_seconds >= 5:
                    self.flush_statistics()
            self._statistics_tick_at = now
            self._statistics_wall_at = wall_at
            self._statistics_was_active = active

    def set_check_counter(self, total, increment):
        with self._lock:
            self._state["profile_checks"] = total
            self._check_counter = increment

    def availability_checked(self):
        with self._lock:
            self._state["checks"] += 1
            if self._check_counter:
                try:
                    self._state["profile_checks"] = self._check_counter()
                except (OSError, ValueError):
                    self._state["statistics_error"] = True
                    self.record("statistics_error", detail="Не удалось сохранить счётчик профиля")
            else:
                self._state["profile_checks"] += 1

    def request_sent(self):
        with self._lock:
            self._state["requests"] += 1

    def finish(self, reason):
        with self._lock:
            self.flush_statistics()
            self._history_mode("finished")
            self._state.update(finished=True, mode="finished", next_action=reason,
                               remaining_seconds=None, updated_at=time.time())

    def snapshot(self):
        with self._lock:
            state = dict(self._state, paused=self._paused, browser_retained=self.browser_retained, booking_handoff=self.booking_handoff, timeline=list(self._timeline), journal=self._journal.snapshot(), availability=list(self._availability))
            remaining = state["remaining_seconds"]
            if remaining is not None and not self._paused:
                state["remaining_seconds"] = max(0, remaining - (time.monotonic() - self._remaining_at))
            if self._paused and not state["finished"]:
                state["mode"] = "paused"
            return state
