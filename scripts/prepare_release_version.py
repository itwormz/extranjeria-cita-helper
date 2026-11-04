"""Promote the version and release notes declared by a merged release PR."""
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def prepare(root=ROOT):
    version = (root / '.github/release-version').read_text().strip()
    if not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', version):
        raise ValueError('Expected a stable version such as 0.3.1')

    replacements = {
        'app/version.py': (r'^VERSION = "[^"]+"$', f'VERSION = "{version}"'),
        'pyproject.toml': (r'^version = "[^"]+"$', f'version = "{version}"'),
        'uv.lock': (r'(?<=^name = "extranjeria-cita-helper"\n)version = "[^"]+"$', f'version = "{version}"'),
    }
    changes = {}
    for filename, (pattern, replacement) in replacements.items():
        path = root / filename
        updated, count = re.subn(pattern, replacement, path.read_text(), flags=re.MULTILINE)
        if count != 1:
            raise ValueError(f'Expected one version in {filename}, found {count}')
        changes[path] = updated

    changelog = root / 'CHANGELOG.md'
    notes = changelog.read_text()
    if re.search(rf'^## {re.escape(version)}(?: |$)', notes, re.MULTILINE):
        raise ValueError(f'Changelog already contains release {version}')
    notes, count = re.subn(r'^## Unreleased(?= — .+$|$)', f'## {version}', notes, flags=re.MULTILINE)
    if count != 1:
        raise ValueError('Expected one ## Unreleased section in CHANGELOG.md')
    if f'## {version}\n' in notes:
        notes = notes.replace(f'## {version}\n', f'## {version} — Release\n', 1)
    changes[changelog] = notes
    for path, content in changes.items():
        path.write_text(content)


if __name__ == '__main__':
    prepare()
