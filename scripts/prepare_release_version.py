"""Promote the version and release notes declared by a release PR before merge."""
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
    sections = list(re.finditer(r'^## Unreleased(?: — (.+))?$', notes, re.MULTILINE))
    if len(sections) != 1:
        raise ValueError('Expected one ## Unreleased section in CHANGELOG.md')
    section = sections[0]
    next_heading = re.search(r'^## ', notes[section.end():], re.MULTILINE)
    end = section.end() + next_heading.start() if next_heading else len(notes)
    body = notes[section.end():end].strip()
    if not body:
        raise ValueError('Add release notes under ## Unreleased before preparing the release')
    title = section.group(1) or 'Release'
    notes = notes[:section.start()] + f'## Unreleased\n\n## {version} — {title}\n\n{body}\n\n' + notes[end:]
    changes[changelog] = notes
    for path, content in changes.items():
        path.write_text(content)


if __name__ == '__main__':
    prepare()
