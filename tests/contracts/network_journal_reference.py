"""Record Python 0.3.6 `IcpNetworkJournal` rows for scripted request/response sequences.

Usage: uv run --frozen --no-sync python -m tests.contracts.network_journal_reference > fixtures/network-journal-0.3.6.json
The real class runs against a fake browser context; wall time and monotonic time are scripted.
"""
import json
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

from app import icp_network_journal as module

WALL = "2026-11-10T10:00:00+01:00"
ICP = "https://icp.administracionelectronica.gob.es/icpplus/"


def scenarios():
    def s(op, at, url=ICP + "acCitar", method="POST", resource_type="document", redirected=False, status=200):
        return dict(op=op, at=at, url=url, method=method, resource_type=resource_type, redirected=redirected, status=status)

    out = []
    out.append(("request_response_failed_rows", [
        s("request", 0, ICP + "index.html", "GET"),
        s("response", 1, ICP + "index.html", "GET", status=200),
        s("request", 2, ICP + "acCitar;jsessionid=ABC", "POST", redirected=True),
        s("failed", 3, ICP + "acVerFormulario/", "GET", "script"),
        s("request", 4, ICP + "static/app.js", "GET", "script"),
        s("request", 5, ICP + "api/data", "POST", "xhr"),
        s("request", 6, ICP + "api/other", "GET", "fetch"),
        s("request", 7, ICP + "logo.png", "GET", "image"),
        s("request", 8, ICP + "site.css", "GET", "stylesheet"),
        s("request", 9, ICP + "font.woff", "GET", "font"),
        s("request", 10, ICP + "misc", "GET", "other"),
    ]))
    out.append(("foreign_hosts_are_ignored", [
        s("request", 0, "https://example.com/icpplus/acCitar", "POST"),
        s("response", 1, "https://example.com/icpplus/acCitar", "POST", status=429),
        s("failed", 2, "https://pasarela.clave.gob.es/x", "GET"),
        s("request", 3, ICP + "acCitar", "POST"),
    ]))
    out.append(("rate_limit_counts_requests_by_minutes", [
        s("request", 0), s("request", 30), s("request", 61), s("request", 125), s("request", 190), s("request", 250),
        s("response", 251, status=429),
        s("request", 299), s("response", 300, status=429),
        s("response", 361, status=429),
        s("response", 900, status=429),
        s("response", 901, status=200),
    ]))
    return out


def run():
    result = []
    for name, steps in scenarios():
        clock = [0.0]
        module.time = SimpleNamespace(monotonic=lambda: clock[0])

        class FakeDateTime:
            @classmethod
            def now(cls):
                return SimpleNamespace(astimezone=lambda: SimpleNamespace(isoformat=lambda: WALL))

        module.datetime = FakeDateTime
        handlers = {}
        context = SimpleNamespace(on=lambda event, handler: handlers.__setitem__(event, handler))
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "icp-network.jsonl"
            module.IcpNetworkJournal().install(context, path, enabled=lambda: True)
            for step in steps:
                clock[0] = step["at"]
                request = SimpleNamespace(url=step["url"], method=step["method"], resource_type=step["resource_type"],
                                          redirected_from=object() if step["redirected"] else None)
                if step["op"] == "request":
                    handlers["request"](request)
                elif step["op"] == "response":
                    handlers["response"](SimpleNamespace(request=request, status=step["status"]))
                else:
                    handlers["requestfailed"](request)
            rows = [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
        result.append(dict(name=name, steps=steps, rows=rows))
    return dict(schema=1, source="v0.3.6", wall=WALL, scenarios=result)


if __name__ == "__main__":
    json.dump(run(), sys.stdout, ensure_ascii=False, indent=1, sort_keys=True)
    sys.stdout.write("\n")
