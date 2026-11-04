from types import SimpleNamespace

from app.icp_network_journal import IcpNetworkJournal


def test_background_requests_share_limit_and_429_diagnostics(tmp_path):
    import json

    handlers = {}
    context = SimpleNamespace(on=handlers.__setitem__)
    path = tmp_path / "icp-network.jsonl"
    limit = IcpNetworkJournal()
    request = SimpleNamespace(
        url="https://icp.administracionelectronica.gob.es/acEntrada;session=secret?token=secret",
        method="GET", resource_type="xhr", redirected_from=None,
    )
    redirect = SimpleNamespace(**vars(request))
    redirect.redirected_from = request

    limit.install(context, path, enabled=lambda: True)
    handlers["request"](request)
    handlers["requestfailed"](request)
    handlers["request"](redirect)
    handlers["response"](SimpleNamespace(request=redirect, status=429))
    raw = path.read_text()
    assert "secret" not in raw
    rows = [json.loads(line) for line in raw.splitlines()]
    assert [row["event"] for row in rows] == ["request", "failed", "request", "response"]
    assert rows[-1]["requests_by_minutes"] == {str(minute): 2 for minute in range(1, 6)}
    assert len(limit.sent) == 2
