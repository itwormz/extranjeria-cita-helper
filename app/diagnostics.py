"""Passive local snapshots for the lifetime of an assist browser page."""

import asyncio
import logging
import hashlib

from .page_inspection import visible_documents
from contextlib import suppress
from urllib.parse import urlsplit

from .logging_utils import log_event

log = logging.getLogger(__name__)


class PageTrace:
    def __init__(self, page, checker, task_id, run_id, document_key):
        self.page = page
        self.checker = checker
        self.task_id = task_id
        self.run_id = run_id
        self.document_key = document_key
        self.pending = asyncio.Event()
        self.saved = set()
        self.attempts = {}
        self.task = None
        self.responses = set()
        self.script_responses = set()
        self.navigation_responses = set()
        self.children = []

    def changed(self, *_args):
        self.pending.set()

    def response(self, response):
        from .diagnostic_resources import MAX_RESPONSE_TASKS, script_url
        navigation = response.request.is_navigation_request()
        if not navigation and (getattr(response.request, "resource_type", "") != "script"
                               or not script_url(response.url)):
            return
        group = self.navigation_responses if navigation else self.script_responses
        if len(group) >= MAX_RESPONSE_TASKS:
            return
        task = asyncio.create_task(self.checker._save_response(response, self.task_id, self.run_id))
        self.responses.add(task)
        group.add(task)
        task.add_done_callback(self.responses.discard)
        task.add_done_callback(group.discard)

    def popup(self, page):
        trace = PageTrace(page, self.checker, self.task_id, self.run_id, self.document_key)
        self.children.append(trace)
        trace.start()

    def start(self):
        self.page.on('domcontentloaded', self.changed)
        self.page.on('framenavigated', self.changed)
        self.page.on('frameattached', self.changed)
        self.page.on('response', self.response)
        self.page.on('popup', self.popup)
        self.pending.set()
        self.task = asyncio.create_task(self._run())

    async def stop(self):
        self.page.remove_listener('domcontentloaded', self.changed)
        self.page.remove_listener('framenavigated', self.changed)
        self.page.remove_listener('frameattached', self.changed)
        self.page.remove_listener('response', self.response)
        self.page.remove_listener('popup', self.popup)
        self.task.cancel()
        with suppress(asyncio.CancelledError):
            await self.task
        for child in self.children:
            await child.stop()
        if self.responses:
            done, pending = await asyncio.wait(self.responses, timeout=5)
            for task in pending:
                task.cancel()
            if pending:
                await asyncio.gather(*pending, return_exceptions=True)

    async def _run(self):
        while True:
            try:
                with suppress(asyncio.TimeoutError):
                    await asyncio.wait_for(self.pending.wait(), timeout=1)
                self.pending.clear()
                if urlsplit(self.page.url).scheme not in {'http', 'https'}:
                    continue
                document = await self.document_key(self.page)
                contents = await visible_documents(self.page)
                fingerprint = hashlib.sha256("\n".join(text for _, text in contents).encode()).hexdigest()
                key = (document, fingerprint)
                if key in self.saved or self.attempts.get(key, 0) >= 2:
                    continue
                self.attempts[key] = self.attempts.get(key, 0) + 1
                saved = await asyncio.wait_for(self.checker._save_debug(
                    self.page, self.task_id, self.run_id,
                    error_code='TRACE_SCREEN', include_controls=True,
                ), timeout=10)
                if saved and document == await self.document_key(self.page):
                    self.saved.add(key)
            except Exception:
                if self.page.is_closed():
                    return
                log_event(log, logging.WARNING, 'assist_trace_failed', task_id=self.task_id,
                          run_id=self.run_id)
