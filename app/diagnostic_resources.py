"""Passive, content-addressed first-party script archive. Never makes requests."""
import gzip
import hashlib
import json
import re
import shutil
import time
from contextlib import contextmanager
from html.parser import HTMLParser
from urllib.parse import urljoin, urlsplit, urlunsplit

MAX_SCRIPT_BYTES = 2 * 1024 * 1024
MAX_RESPONSE_TASKS = 8
HOST = "icp.administracionelectronica.gob.es"


def script_url(url):
    try:
        parts = urlsplit(url)
        if parts.scheme != "https" or parts.hostname != HOST or parts.port not in (None, 443):
            return None
        path = re.sub(r";[^/]*", "", parts.path)
        if not path.startswith(("/icpplus/", "/icpplustieb/")):
            return None
        return urlunsplit(("https", HOST, path, "", ""))
    except (ValueError, TypeError):
        return None


def script_reference(url):
    # Hash exact requested URL to distinguish query versions without saving tokens.
    return hashlib.sha256(url.split("#", 1)[0].encode("utf-8")).hexdigest()


@contextmanager
def archive_lock(root):
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    from .locking import locked_file
    with locked_file(root / ".resources.lock"):
        yield


def _manifest(directory):
    path = directory / "resources.json"
    records = json.loads(path.read_text()) if path.exists() else []
    if not isinstance(records, list):
        raise ValueError("Invalid resource manifest")
    for record in records:
        if not isinstance(record, dict) or any(
            not isinstance(record.get(key), str) or re.fullmatch(r"[0-9a-f]{64}", record[key]) is None
            for key in ("sha256", "reference")
        ):
            raise ValueError("Invalid resource manifest record")
    return records


def save_script(directory, url, body, status, content_type):
    safe_url = script_url(url)
    if not safe_url or not body or len(body) > MAX_SCRIPT_BYTES:
        return False
    mime = content_type.lower().split(";", 1)[0].strip()
    if mime not in {"application/javascript", "text/javascript", "application/x-javascript", "application/ecmascript", "text/ecmascript"}:
        return False
    if body.lstrip().lower().startswith((b"<", b"<!doctype")) or b"requestrejected" in body[:4096].replace(b" ", b"").lower():
        return False
    root = directory.parent.parent
    digest = hashlib.sha256(body).hexdigest()
    with archive_lock(root):
        directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        objects = root / "resources"
        objects.mkdir(exist_ok=True, mode=0o700)
        target = objects / (digest + ".js.gz")
        if not target.exists():
            temporary = target.with_suffix(".tmp")
            temporary.write_bytes(gzip.compress(body, mtime=0))
            temporary.chmod(0o600)
            temporary.replace(target)
        records = _manifest(directory)
        reference = script_reference(url)
        record = next((item for item in records if item.get("reference") == reference and item["sha256"] == digest), None)
        now = time.time()
        if record is None:
            record = {"url": safe_url, "reference": reference, "sha256": digest,
                      "first_seen": now, "load_count": 0}
            records.append(record)
        record.update(last_seen=now, load_count=record["load_count"] + 1,
                      status=status, content_type=mime)
        path = directory / "resources.json"
        temporary = path.with_suffix(".tmp")
        temporary.write_text(json.dumps(records, indent=2), encoding="utf-8")
        temporary.chmod(0o600)
        temporary.replace(path)
    return digest


def prepare_session(root, session_name, retention_days, *, cleanup=True):
    directory = root / "sessions" / session_name
    with archive_lock(root):
        directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        directory.touch()
        if not cleanup:
            return directory
        cutoff = time.time() - retention_days * 86400
        for old in directory.parent.iterdir():
            if old != directory and not old.is_symlink() and old.is_dir() and old.stat().st_mtime < cutoff:
                shutil.rmtree(old)
        referenced = set()
        # A malformed kept manifest must never cause deletion of its objects.
        try:
            for session in directory.parent.iterdir():
                if session.is_dir() and not session.is_symlink():
                    referenced.update(record["sha256"] for record in _manifest(session))
        except (ValueError, KeyError, OSError):
            return directory
        for obj in (root / "resources").glob("*.js.gz"):
            if obj.name[:-6] not in referenced:
                obj.unlink()
    return directory


class _Scripts(HTMLParser):
    def __init__(self):
        super().__init__()
        self.sources = []
        self.base=None
    def handle_starttag(self, tag, attrs):
        if tag=='base' and self.base is None:
            self.base=dict(attrs).get('href')
        if tag == "script":
            source = dict(attrs).get("src")
            if source:
                self.sources.append(source)


def snapshot_references(directory, documents):
    """Reference exact script URLs, including loads whose body is still pending.

    Missing references can later be joined to the session manifest by reference.
    """
    with archive_lock(directory.parent.parent):
        records = _manifest(directory)
    latest = {record.get("reference"): record for record in sorted(records, key=lambda item: item.get("last_seen", 0))}
    result = []
    for url, html in documents:
        parser = _Scripts()
        parser.feed(html)
        try:base=urljoin(url,parser.base) if parser.base else url
        except ValueError:base=url
        for source in parser.sources:
            try:joined=urljoin(base,source)
            except ValueError:continue
            safe = script_url(joined)
            if safe:
                reference = script_reference(joined)
                record = latest.get(reference)
                digest = record["sha256"] if record else None
                result.append({"url": safe, "reference": reference, "sha256": digest,
                               "state": "captured" if digest else "missing"})
    return result
