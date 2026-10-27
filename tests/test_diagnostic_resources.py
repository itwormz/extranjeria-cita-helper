import asyncio
import gzip
import json
import os
import time
from types import SimpleNamespace
from unittest.mock import AsyncMock

import pytest

from app.diagnostic_resources import (MAX_SCRIPT_BYTES, prepare_session, save_script,
                                      snapshot_references)

URL = "https://icp.administracionelectronica.gob.es/icpplus/js/main.js"


def test_content_deduplicates_across_sessions_urls_and_versions(tmp_path):
    first = prepare_session(tmp_path, "first", 30)
    second = prepare_session(tmp_path, "second", 30)
    assert save_script(first, URL + "?token=secret", b"let a=1", 200, "text/javascript")
    assert save_script(second, URL.replace("main", "other"), b"let a=1", 200, "text/javascript")
    assert len(list((tmp_path / "resources").glob("*.gz"))) == 1
    assert save_script(first, URL, b"let a=2", 200, "application/javascript; charset=utf-8")
    assert len(list((tmp_path / "resources").glob("*.gz"))) == 2
    assert "secret" not in (first / "resources.json").read_text()
    refs = snapshot_references(first, [(URL, '<script src="main.js"></script><script src="missing.js"></script>')])
    records = json.loads((first / "resources.json").read_text())
    assert refs[0]["sha256"] == records[-1]["sha256"]
    assert refs[1]["state"] == "missing"
    assert gzip.decompress(next((tmp_path / "resources").glob("*.gz")).read_bytes()) in (b"let a=1", b"let a=2")


@pytest.mark.parametrize("url,body,mime", [
    ("https://pasarela.clave.gob.es/main.js", b"a()", "text/javascript"),
    (URL, b"<html>Request Rejected</html>", "text/javascript"),
    (URL, b"a()", "text/html"),
    (URL, b"x" * (MAX_SCRIPT_BYTES + 1), "text/javascript"),
], ids=["external-host", "html-body", "html-mime", "oversize"])
def test_rejects_external_html_and_oversize(tmp_path, url, body, mime):
    directory = prepare_session(tmp_path, "test", 30)
    assert not save_script(directory, url, body, 200, mime)
    assert not (directory / "resources.json").exists()


def test_retention_preserves_shared_objects_and_collects_unreferenced(tmp_path):
    old = prepare_session(tmp_path, "old", 30)
    kept = prepare_session(tmp_path, "kept", 30)
    save_script(old, URL, b"shared()", 200, "text/javascript")
    save_script(kept, URL, b"shared()", 200, "text/javascript")
    save_script(old, URL, b"oldOnly()", 200, "text/javascript")
    past = time.time() - 86400 * 40
    os.utime(old, (past, past))
    prepare_session(tmp_path, "kept", 30)
    assert not old.exists()
    objects = list((tmp_path / "resources").glob("*.gz"))
    assert len(objects) == 1
    assert gzip.decompress(objects[0].read_bytes()) == b"shared()"


def test_capture_disabled_external_and_cancelled(tmp_path, monkeypatch):
    from app.config import Settings
    from app.site import IcpPlusChecker
    monkeypatch.setenv("DEBUG_DIR", str(tmp_path))
    checker = IcpPlusChecker(Settings(diagnostics_enabled=False))
    response = SimpleNamespace(url=URL, status=200, headers={"content-type": "text/javascript"},
                               body=AsyncMock(return_value=b"main()"),
                               request=SimpleNamespace(resource_type="script", is_navigation_request=lambda: False))
    asyncio.run(checker._save_response(response, "test", "run"))
    response.body.assert_not_awaited()
    checker = IcpPlusChecker(Settings(diagnostics_enabled=True))
    response.url = "https://pasarela.clave.gob.es/main.js"
    asyncio.run(checker._save_response(response, "test", "run"))
    response.body.assert_not_awaited()
    response.url = URL
    response.body = AsyncMock(side_effect=asyncio.CancelledError)
    with pytest.raises(asyncio.CancelledError):
        asyncio.run(checker._save_response(response, "test", "run"))
    assert not list(tmp_path.rglob("*.gz"))


def test_script_response_is_archived_and_no_sensitive_metadata_written(tmp_path, monkeypatch):
    from app.config import Settings
    from app.site import IcpPlusChecker
    monkeypatch.setenv("DEBUG_DIR", str(tmp_path))
    checker = IcpPlusChecker(Settings(diagnostics_enabled=True))
    response = SimpleNamespace(url=URL + ";jsessionid=private?token=secret", status=200,
        headers={"content-type": "application/javascript", "set-cookie": "private-cookie"},
        body=AsyncMock(return_value=b"console.log('loaded')"),
        request=SimpleNamespace(resource_type="script", is_navigation_request=lambda: False))
    asyncio.run(checker._save_response(response, "test", "run"))
    manifest = next(tmp_path.rglob("resources.json")).read_text()
    assert "private" not in manifest and "secret" not in manifest and "cookie" not in manifest
    assert len(list(tmp_path.rglob("*.js.gz"))) == 1


def test_trace_response_tasks_are_bounded_and_stop_cancels_pending():
    from app.diagnostics import PageTrace
    from app.diagnostic_resources import MAX_RESPONSE_TASKS
    async def scenario():
        started = asyncio.Event()
        async def save(*args):
            started.set()
            await asyncio.Event().wait()
        page = SimpleNamespace(remove_listener=lambda *args: None)
        trace = PageTrace(page, SimpleNamespace(_save_response=save), "test", "run", None)
        # Model a running trace without starting polling or touching a browser.
        trace.task = asyncio.create_task(asyncio.Event().wait())
        for _ in range(MAX_RESPONSE_TASKS + 5):
            trace.response(SimpleNamespace(url=URL, request=SimpleNamespace(resource_type="script", is_navigation_request=lambda: False)))
        assert len(trace.responses) == MAX_RESPONSE_TASKS
        await started.wait()
        for task in list(trace.responses):
            task.cancel()
        await trace.stop()
        assert not trace.responses
    asyncio.run(scenario())


def test_snapshot_does_not_substitute_different_query_version(tmp_path):
    directory = prepare_session(tmp_path, "test", 30)
    digest = save_script(directory, URL + "?v=1", b"versionOne()", 200, "text/javascript")
    documents = [(URL, '<script src="main.js?v=2"></script>')]
    refs = snapshot_references(directory, documents)
    assert refs[0]["url"] == URL and refs[0]["sha256"] is None and refs[0]["state"] == "missing"


@pytest.mark.parametrize("url", ["https://icp.administracionelectronica.gob.es:bad/icpplus/a.js", "https://[invalid/icpplus/a.js"])
def test_invalid_script_urls_are_ignored(url):
    from app.diagnostic_resources import script_url
    assert script_url(url) is None


def test_tieb_capture_and_duplicate_load_manifest_are_bounded(tmp_path):
    url = URL.replace("/icpplus/", "/icpplustieb/") + "?v=first"
    directory = prepare_session(tmp_path, "test", 30)
    for _ in range(20):
        save_script(directory, url, b"loaded()", 200, "text/javascript")
    records = json.loads((directory / "resources.json").read_text())
    assert len(records) == 1 and records[0]["load_count"] == 20
    assert records[0]["last_seen"] >= records[0]["first_seen"]
    save_script(directory, url.replace("first", "second"), b"loaded()", 200, "text/javascript")
    records = json.loads((directory / "resources.json").read_text())
    assert len(records) == 2 and records[0]["reference"] != records[1]["reference"]


def test_snapshot_before_script_capture_has_joinable_reference(tmp_path):
    url = URL + "?token=private&v=2"
    directory = prepare_session(tmp_path, "test", 30)
    refs = snapshot_references(directory, [(URL, '<script src="main.js?token=private&amp;v=2"></script>')])
    assert refs[0]["state"] == "missing"
    digest = save_script(directory, url, b"late()", 200, "text/javascript")
    records = json.loads((directory / "resources.json").read_text())
    match = next(record for record in records if record["reference"] == refs[0]["reference"])
    assert match["sha256"] == digest
    assert "private" not in json.dumps(refs) + json.dumps(records)


def test_irrelevant_response_burst_and_full_script_queue_preserve_navigation():
    from app.diagnostics import PageTrace
    from app.diagnostic_resources import MAX_RESPONSE_TASKS
    async def scenario():
        blocker = asyncio.Event()
        async def capture(*args):
            await blocker.wait()
        trace = PageTrace(None, SimpleNamespace(_save_response=capture), "test", "run", None)
        image = SimpleNamespace(url=URL, request=SimpleNamespace(resource_type="image", is_navigation_request=lambda: False))
        for _ in range(100):
            trace.response(image)
        assert not trace.responses
        script = SimpleNamespace(url=URL, request=SimpleNamespace(resource_type="script", is_navigation_request=lambda: False))
        for _ in range(MAX_RESPONSE_TASKS + 5):
            trace.response(script)
        navigation = SimpleNamespace(request=SimpleNamespace(is_navigation_request=lambda: True))
        trace.response(navigation)
        assert len(trace.script_responses) == MAX_RESPONSE_TASKS
        assert len(trace.navigation_responses) == 1
        blocker.set()
        await asyncio.gather(*trace.responses)
    asyncio.run(scenario())


def test_prepare_session_can_skip_cleanup_scan(tmp_path, monkeypatch):
    directory = prepare_session(tmp_path, "test", 30)
    monkeypatch.setattr(type(directory), "iterdir", lambda self: (_ for _ in ()).throw(AssertionError("unexpected scan")))
    assert prepare_session(tmp_path, "test", 30, cleanup=False) == directory


@pytest.mark.parametrize("malformed", [{}, [None], [{"sha256": "invalid", "reference": "invalid"}]])
def test_malformed_kept_manifest_prevents_gc_without_raising(tmp_path, malformed):
    kept = prepare_session(tmp_path, "kept", 30)
    save_script(kept, URL, b"keepEvenWhenManifestIsCorrupt()", 200, "text/javascript")
    objects = list((tmp_path / "resources").glob("*.gz"))
    (kept / "resources.json").write_text(json.dumps(malformed))
    assert prepare_session(tmp_path, "new", 30).is_dir()
    assert all(obj.exists() for obj in objects)


def test_script_url_normalizes_session_parameter_on_context_path():
    from app.diagnostic_resources import script_url
    assert script_url("https://icp.administracionelectronica.gob.es/icpplustieb;jsessionid=private/resources/a.js") == "https://icp.administracionelectronica.gob.es/icpplustieb/resources/a.js"


def test_snapshot_resolves_script_against_html_base(tmp_path):
    from app.diagnostic_resources import script_reference
    directory=prepare_session(tmp_path,'base',30)
    target='https://icp.administracionelectronica.gob.es/icpplustieb/resources/js/main.js'
    digest=save_script(directory,target,b'baseExample()',200,'text/javascript')
    refs=snapshot_references(directory,[('https://icp.administracionelectronica.gob.es/icpplus/nested/page','<base href="/icpplustieb/resources/"><script src="js/main.js"></script>')])
    assert refs[0]['reference']==script_reference(target)
    assert refs[0]['sha256']==digest


@pytest.mark.parametrize('html',[
    '<base href="//[bad/"><script src="main.js"></script>',
    '<script src="//[bad/x.js"></script><script src="main.js"></script>',
])
def test_snapshot_references_skip_malformed_base_and_script_urls(tmp_path,html):
    directory=prepare_session(tmp_path,'test',30)
    refs=snapshot_references(directory,[(URL,html)])
    assert [row['url'] for row in refs]==[URL]
