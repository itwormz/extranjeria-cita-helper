"""Check public source and Git history for private files and personal values."""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import re
import subprocess

import yaml


def git(*args: str) -> bytes:
    return subprocess.check_output(['git', *args])


def sensitive_values(project: Path) -> dict[str, str]:
    values = {}
    tasks = project / 'tasks.yml'
    if tasks.exists():
        config = yaml.safe_load(tasks.read_text(encoding='utf-8')) or {}
        for task in config.get('tasks', []):
            applicant = task.get('applicant', {})
            entries = [(key, applicant.get(key)) for key in ('document_number', 'full_name')]
            entries += [('contact_' + key, value) for key, value in applicant.get('contact', {}).items()]
            entries += [('chat_id', value) for value in task.get('notify', {}).get('chat_ids', [])]
            for label, value in entries:
                if value is not None and len(str(value).strip()) >= 5:
                    values[str(value).strip().casefold()] = label
    env = project / '.env'
    if env.exists():
        for line in env.read_text(encoding='utf-8').splitlines():
            key, separator, value = line.partition('=')
            if separator and not key.lstrip().startswith('#') and any(
                word in key.upper() for word in ('TOKEN', 'SECRET', 'PASSWORD')
            ):
                value = value.strip().strip('\"\'')
                if len(value) >= 8:
                    values[value.casefold()] = 'env_secret'
    return values


def matches(data: bytes, values: dict[str, str]) -> list[str]:
    text = data.decode('utf-8', errors='replace').casefold()
    labels = {label for value, label in values.items() if value in text}
    if re.search(rb'-----BEGIN (?:RSA |EC |OPENSSH |ENCRYPTED )?PRIVATE KEY-----', data):
        labels.add('private_key')
    return sorted(labels)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--history', action='store_true')
    parser.add_argument('--tree', type=Path, help='Check source files before creating a Git repository')
    parser.add_argument('--personal-source', type=Path, help='Local profile directory to compare against; values are never printed')
    args = parser.parse_args()
    if args.tree:
        project = args.tree.resolve()
        values = sensitive_values(args.personal_source or project)
        findings = 0; checked = 0
        excluded = {'.git', '.venv', 'data', 'debug', '__pycache__', '.pytest_cache', '.local', 'build', 'dist'}
        for file in project.rglob('*'):
            relative = file.relative_to(project)
            if any(part in excluded or part.endswith('.egg-info') for part in relative.parts) or not file.is_file():
                continue
            labels = matches(file.read_bytes(), values)
            if relative.name in {'.env', 'tasks.yml'} or relative.suffix.lower() in {'.p12','.pfx','.key','.db','.sqlite','.sqlite3','.log'} or relative.parts[0] in {'certs','captures','browser-profiles','.html'}:
                labels.append('private_path')
            checked += 1
            if labels:
                findings += 1; print(f'SOURCE {relative}: {", ".join(labels)}')
        print(f'Source: {checked} files, {findings} findings.')
        return int(findings > 0)
    root = Path(git('rev-parse', '--show-toplevel').decode().strip())
    os.chdir(root)
    project = root
    prefix = str(project.relative_to(root))
    values = sensitive_values(args.personal_source or project)
    if not values:
        print('No local personal values loaded; only path/private-key checks are available.')
    failures = 0
    checked = 0
    for row in git('ls-files', '--stage', '-z', '--', prefix).split(b'\0'):
        if not row:
            continue
        meta, name = row.split(b'\t', 1)
        oid = meta.split()[1].decode()
        path = name.decode()
        relative = Path(path).relative_to(prefix)
        labels = matches(git('cat-file', '-p', oid), values)
        if relative.name in {'.env', 'tasks.yml'} or relative.parts[0] in {
            'data', 'debug', 'certs', 'browser-profiles', 'captures',
        } or relative.suffix.lower() in {'.p12', '.pfx', '.key', '.db', '.sqlite', '.sqlite3', '.log'}:
            labels.append('private_path')
        checked += 1
        if labels:
            failures += 1
            print(f'INDEX {path}: {", ".join(labels)}')
    print(f'Index: {checked} files, {failures} findings.')
    if args.history:
        seen = set()
        for row in git('rev-list', '--objects', '--all', '--', prefix).decode().splitlines():
            oid, _, path = row.partition(' ')
            if oid in seen or git('cat-file', '-t', oid).strip() != b'blob':
                continue
            seen.add(oid)
            labels = matches(git('cat-file', '-p', oid), values)
            if labels:
                failures += 1
                print(f'HISTORY {oid[:12]} {path}: {", ".join(labels)}')
        print(f'History: {len(seen)} blobs inspected.')
    return int(failures > 0)


if __name__ == '__main__':
    raise SystemExit(main())
