"""Check documentation markup and local file links without network requests."""
from __future__ import annotations

from html.parser import HTMLParser
from pathlib import Path
import re
from urllib.parse import unquote

from markdown_it import MarkdownIt


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
    parser = MarkdownIt('commonmark').enable('table')
    tokens = parser.parse(text)
    lines = text.splitlines()
    errors = []
    for token in tokens:
        if token.type == 'fence':
            last = lines[token.map[1] - 1]
            closing = re.fullmatch(r'[ >\t]*(' + re.escape(token.markup[0])
                                   + '{' + str(len(token.markup)) + r',})\s*', last)
            if token.map[1] - token.map[0] < 2 or closing is None:
                errors.append('Unclosed fenced code block')
    # Rendering lets the CommonMark parser handle escaped/balanced link
    # destinations and keep fenced, indented and inline code as literal text.
    html = Links()
    html.feed(parser.renderer.render(tokens, parser.options, {}))
    errors.extend(html.errors)
    if html.details:
        errors.append('Unclosed details tag')
    targets = html.targets
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
