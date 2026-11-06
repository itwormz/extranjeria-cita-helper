"""Check documentation markup and local file links without network requests."""
from __future__ import annotations

from html.parser import HTMLParser
from pathlib import Path
import re
from urllib.parse import unquote


class Links(HTMLParser):
    def __init__(self):
        super().__init__()
        self.targets = []
        self.details = 0
        self.errors = []

    def handle_starttag(self, tag, attributes):
        attrs = dict(attributes)
        for name in ('href', 'src'):
            if attrs.get(name):
                self.targets.append(attrs[name])
        if attrs.get('srcset'):
            self.targets.extend(item.strip().split()[0]
                                for item in attrs['srcset'].split(',') if item.strip())
        if tag == 'details':
            self.details += 1

    def handle_endtag(self, tag):
        if tag == 'details':
            if self.details:
                self.details -= 1
            else:
                self.errors.append('Closing details tag without an opening tag')


def check_document(path: Path) -> list[str]:
    text = path.read_text(encoding='utf-8')
    lines = []
    fence = None
    for line in text.splitlines():
        marker = re.match(r'^ {0,3}(`{3,}|~{3,})(.*)$', line)
        if marker:
            token, rest = marker.groups()
            if fence is None:
                fence = token
            elif token[0] == fence[0] and len(token) >= len(fence) and not rest.strip():
                fence = None
            continue
        if fence is None:
            lines.append(line)
    errors = ['Unclosed fenced code block'] if fence else []
    visible = re.sub(r'(`+).*?\1', '', '\n'.join(lines))
    html = Links()
    html.feed(visible)
    errors.extend(html.errors)
    if html.details:
        errors.append('Unclosed details tag')
    targets = html.targets
    targets.extend(match.group(1) or match.group(2) for match in re.finditer(
        r'\]\(\s*(?:<([^>]+)>|([^\s)]+))', visible))
    targets.extend(match.group(1) or match.group(2) for match in re.finditer(
        r'^ {0,3}\[[^\]]+\]:\s*(?:<([^>]+)>|(\S+))', visible, re.MULTILINE))
    for target in sorted(set(targets)):
        if target.startswith(('#', '//')) or re.match(r'^[a-zA-Z][a-zA-Z0-9+.-]*:', target):
            continue
        local = unquote(target.split('#', 1)[0].split('?', 1)[0])
        if local and not (path.parent / local).exists():
            errors.append('Missing local link or image: ' + target)
    return errors


def main():
    root = Path.cwd()
    files = sorted(set(root.glob('README*.md')) | {
        path for path in (root / 'docs').rglob('*')
        if path.is_file() and path.suffix.lower() == '.md'})
    changelog = root / 'CHANGELOG.md'
    if changelog.exists():
        files.append(changelog)
    failures = 0
    for path in files:
        for message in check_document(path):
            print(f'{path.relative_to(root)}: {message}')
            failures += 1
    print(f'Documentation: {len(files)} files, {failures} errors (local links and basic markup).')
    return int(failures > 0)


if __name__ == '__main__':
    raise SystemExit(main())
