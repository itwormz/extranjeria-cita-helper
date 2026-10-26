"""Local release gate; does not commit, push, tag or publish anything."""
import argparse
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from app.version import VERSION


def check_versions(root=ROOT):
    for name in ('pyproject.toml', 'uv.lock'):
        data = tomllib.loads((root / name).read_text(encoding='utf-8'))
        version = (data['project']['version'] if name == 'pyproject.toml' else
                   next(p['version'] for p in data['package'] if p['name'] == 'extranjeria-cita-helper'))
        if version != VERSION:
            raise ValueError(f'{name}: {version} differs from app.version {VERSION}')
    for name in ('README.md', 'README.ru.md', 'README.es.md'):
        readme = (root / name).read_text(encoding='utf-8')
        if ('https://img.shields.io/github/v/release/itwormz/extranjeria-cita-helper?' not in readme
                or 'https://github.com/itwormz/extranjeria-cita-helper/releases/latest' not in readme):
            raise ValueError(f'{name}: latest published release links are missing')
    if f'## {VERSION} ' not in (root / 'CHANGELOG.md').read_text(encoding='utf-8'):
        raise ValueError('Current version is missing from CHANGELOG.md')


def run(args, env=None):
    print('+ ' + ' '.join(map(str, args)), flush=True)
    subprocess.run(args, cwd=ROOT, env=env, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--metadata-only', action='store_true', help='Check release version consistency only')
    parser.add_argument('--bundle', action='store_true', help='Also build and smoke-test the native app on this OS')
    args = parser.parse_args()
    check_versions()
    if args.metadata_only:
        print(f'Release metadata: {VERSION} OK')
        return
    actionlint = shutil.which('actionlint')
    if not actionlint:
        raise SystemExit('actionlint is required. Install it (macOS: brew install actionlint) and rerun.')
    run([actionlint, '-shellcheck='])
    run([sys.executable, 'scripts/check_privacy.py', '--tree', '.'])
    run([sys.executable, '-m', 'pytest'])
    env = dict(os.environ, CITA_BROWSER_TESTS='1', CITA_TEST_ENGINE='camoufox', PYTHONUNBUFFERED='1')
    base = [sys.executable, '-m', 'pytest', '-x', '-v', '-o', 'addopts=', '--tb=short', '--durations=10', '-o', 'faulthandler_timeout=120']
    if platform.system() == 'Darwin':
        run(base + ['tests/test_local_assist_browser.py', '-k', 'locked_desktop'], env)
    run(base + ['tests/test_local_assist_browser.py', 'tests/test_contact_fields_browser.py',
                'tests/test_procedure_groups_browser.py', 'tests/test_regreso_browser.py', 'tests/test_resource_capture_browser.py', '-k', 'not locked_desktop'], env)
    if args.bundle:
        run([sys.executable, 'scripts/build_desktop_app.py'])
        suffix = '.app' if platform.system() == 'Darwin' else ''
        run([sys.executable, 'scripts/verify_bundle.py', 'data/dist/Cita Extranjeria Helper' + suffix])
    print('Local preflight passed. Other operating systems and installers still require native CI checks.')


if __name__ == '__main__':
    main()
