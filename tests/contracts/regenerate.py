"""Rewrite the `expected` side of a golden file from the Python implementation checked out in `app/`.

Usage: uv run python -m tests.contracts.regenerate NAME [NAME...]
NAME is one of: exports, statistics-queries, sqlite-migration, timings. Inputs are never changed; review
`git diff tests/contracts/fixtures` afterwards: only intended behaviour changes may appear.
"""
import json
import sys
import tempfile
from dataclasses import asdict
from pathlib import Path

FIXTURES = Path(__file__).parent / 'fixtures'


def exports(case, directory):
    from .export_reference import replay
    return replay(case['input'], directory / 'fixture.ics')


def statistics_queries(case, directory):
    from .query_reference import replay
    return replay(case['input'], directory / 'statistics.json')


def sqlite_migration(case, directory):
    from .sqlite_reference import replay
    return replay(case['source'], directory / 'stats.json')


def timings(case, directory):
    from app.timings import parse_timings
    if not case['expected']['valid']:
        try:
            parse_timings(case['input'])
        except (ValueError, TypeError):
            return case['expected']
        raise SystemExit(f"case {case['id']} became valid in Python; decide by hand")
    value = asdict(parse_timings(case['input']))
    return {'valid': True, 'value': {k: list(v) if isinstance(v, tuple) else v for k, v in value.items()}}


TARGETS = {
    'exports': ('exports.json', exports, 'expected'),
    'statistics-queries': ('statistics-queries.json', statistics_queries, 'expected'),
    'sqlite-migration': ('sqlite-migration.json', sqlite_migration, 'expected'),
    'timings': ('timings.json', timings, 'expected'),
}


def main(names):
    for name in names:
        file, function, key = TARGETS[name]
        path = FIXTURES / file
        data = json.loads(path.read_text(encoding='utf-8'))
        for case in data['cases']:
            with tempfile.TemporaryDirectory() as directory:
                case[key] = function(case, Path(directory))
        path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        print('regenerated', file, len(data['cases']), 'cases')


if __name__ == '__main__':
    main(sys.argv[1:])
