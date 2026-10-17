"""Offline packaged browser and inherited worker-pipe verification for CI."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def executable(bundle):
    if sys.platform == 'darwin':
        return bundle / 'Contents/Helpers/Cita Worker.app/Contents/MacOS/Cita Extranjeria Helper'
    return bundle / ('Cita Extranjeria Helper.exe' if sys.platform == 'win32' else 'Cita Extranjeria Helper')


def verify(bundle):
    resources = bundle / ('Contents/Resources' if sys.platform == 'darwin' else '_internal')
    assert not (resources / 'camoufox/gui').exists(), 'Unused Camoufox manager was bundled'
    assert not (resources / 'PIL').exists(), 'Unused Pillow was bundled'
    assert not (resources / 'playwright/driver/package/types').exists(), 'Developer types were bundled'
    translations = resources / 'PySide6/Qt/translations'
    assert all(path.stem.rsplit('_', 1)[-1] in {'en', 'es', 'ru'} for path in translations.glob('*.qm')), 'Unused translations were bundled'
    qt = bundle / ('Contents/Frameworks/PySide6' if sys.platform == 'darwin' else '_internal/PySide6')
    assert not any(qt.glob('QtQml*')) and not any(qt.glob('QtQuick*')), 'Unused QML modules were bundled'
    with tempfile.TemporaryDirectory(prefix='cita-offline-') as private:
        env = dict(os.environ, CITA_DATA_HOME=private, QT_QPA_PLATFORM='offscreen')
        program = str(executable(bundle))
        smoke = subprocess.run([program, '--smoke-test'], env=env, capture_output=True, timeout=120)
        assert smoke.returncode == 0, smoke.stderr.decode('utf-8', errors='replace')
        records = [json.loads(line) for line in smoke.stdout.decode('utf-8').splitlines() if line.startswith('{')]
        assert any(record.get('ok') and record.get('browser') for record in records), smoke.stdout
        assert any(record.get('ok') and record.get('gui') for record in records), smoke.stdout
        commands = b'{"command":"pause","paused":true}\n{"command":"stop"}\n'
        ipc = subprocess.run([program, '--ipc-smoke-test'], input=commands, env=env, capture_output=True, timeout=15)
        assert ipc.returncode == 0, ipc.stderr.decode('utf-8', errors='replace')
        records = [json.loads(line) for line in ipc.stdout.decode('utf-8').splitlines() if line.startswith('{')]
        assert records[-1] == {'ok': True, 'paused': True, 'stopped': True}, ipc.stdout
        print('Trimmed package, offline browser, Widgets GUI and worker IPC: OK')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('bundle', type=Path)
    verify(parser.parse_args().bundle.resolve())
