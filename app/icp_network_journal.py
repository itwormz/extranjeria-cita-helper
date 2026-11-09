"""Passive network diagnostics to the appointment site."""
from collections import deque
from datetime import datetime
import json
import logging
import os
import time

from .site import _flow_endpoint, _is_icp_page

log = logging.getLogger(__name__)


class IcpNetworkJournal:
    def __init__(self):
        self.sent = deque()

    def prune(self, now):
        while self.sent and self.sent[0] <= now - 300:
            self.sent.popleft()

    def install(self, context, path=None, enabled=lambda: False):
        def record(event, request, **fields):
            if path is None or not enabled():
                return
            row = dict(time=datetime.now().astimezone().isoformat(), event=event,
                       method=request.method, endpoint=_flow_endpoint(request.url),
                       resource_type=request.resource_type, **fields)
            try:
                path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
                fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
                with os.fdopen(fd, "a", encoding="utf-8") as stream:
                    stream.write(json.dumps(row) + "\n")
            except OSError:
                log.warning("Could not write ICP network diagnostics")

        def response_received(response):
            if not _is_icp_page(response.request.url):
                return
            fields = {"status": response.status}
            if response.status == 429:
                now = time.monotonic()
                self.prune(now)
                fields["requests_by_minutes"] = {
                    minute: sum(now - timestamp < minute * 60 for timestamp in self.sent)
                    for minute in range(1, 6)
                }
            record("response", response.request, **fields)

        def request_failed(request):
            if _is_icp_page(request.url):
                record("failed", request)

        def requested(request):
            if not _is_icp_page(request.url):
                return
            now = time.monotonic()
            self.prune(now)
            self.sent.append(now)
            record("request", request, redirected=bool(request.redirected_from))

        context.on("request", requested)
        context.on("response", response_received)
        context.on("requestfailed", request_failed)
