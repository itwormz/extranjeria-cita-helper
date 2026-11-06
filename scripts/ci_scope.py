"""Conservatively classify a whole PR or main push for lightweight documentation CI."""
from __future__ import annotations

import fnmatch
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess


DOC_EXTENSIONS = {'.md', '.png', '.jpg', '.jpeg', '.gif', '.webp', '.svg'}


def documentation_path(name: str) -> bool:
    if name.startswith('/') or any(part in {'..', '.'} for part in name.split('/')):
        return False
    path = PurePosixPath(name)
    if len(path.parts) == 1:
        return name in {'CHANGELOG.md', 'LICENSE'} or fnmatch.fnmatchcase(name, 'README*.md')
    return path.parts[0] == 'docs' and path.suffix.lower() in DOC_EXTENSIONS


def classify(event_name: str, event: dict, repository: Path) -> bool:
    if event_name == 'pull_request':
        pull = event.get('pull_request', {})
        base = pull.get('base', {}).get('sha', '')
        head = pull.get('head', {}).get('sha', '')
        separator = '...'
    elif event_name == 'push':
        base, head = event.get('before', ''), event.get('after', '')
        separator = '..'
    else:
        return False
    if not all(isinstance(sha, str) and re.fullmatch('[0-9a-f]{40}', sha)
               and sha != '0' * 40 for sha in (base, head)):
        return False
    # PRs use their merge base, never only the most recent commit. Disabling
    # rename detection exposes both source and destination paths for moves.
    changed = subprocess.check_output([
        'git', '-C', str(repository), 'diff', '--no-renames', '--name-only', '-z',
        base + separator + head, '--'])
    paths = [os.fsdecode(path) for path in changed.split(b'\0') if path]
    return bool(paths) and all(documentation_path(path) for path in paths)


def main():
    event = json.loads(Path(os.environ['GITHUB_EVENT_PATH']).read_text(encoding='utf-8'))
    docs_only = classify(os.environ['GITHUB_EVENT_NAME'], event, Path.cwd())
    value = str(docs_only).lower()
    with Path(os.environ['GITHUB_OUTPUT']).open('a', encoding='utf-8') as output:
        output.write(f'docs_only={value}\n')
    message = ('Documentation-only change: lightweight documentation/privacy checks; '
               'application tests and CodeQL analysis are not needed.' if docs_only else
               'Code, configuration or unknown change scope: run normal checks.')
    print(message)
    summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if summary:
        with Path(summary).open('a', encoding='utf-8') as output:
            output.write(message + '\n')


if __name__ == '__main__':
    main()
