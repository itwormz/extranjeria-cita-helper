"""Keep packaged resources separate from writable, private application data."""
import os
from pathlib import Path
import sys

APP_NAME = 'Cita Extranjeria Helper'


def portable_mode():
    return bool(getattr(sys,'frozen',False) and sys.platform=='win32' and
                (Path(sys.executable).parent/'portable.flag').is_file())


def runtime_root():
    if getattr(sys, 'frozen', False):
        if os.environ.get('CITA_DATA_HOME'):
            return Path(os.environ['CITA_DATA_HOME'])
        if portable_mode():
            return Path(sys.executable).parent/'data'
        if sys.platform == 'win32':
            base = Path(os.environ.get('LOCALAPPDATA', Path.home() / 'AppData' / 'Local'))
        elif sys.platform == 'darwin':
            base = Path.home() / 'Library' / 'Application Support'
        else:
            base = Path(os.environ.get('XDG_DATA_HOME', Path.home() / '.local' / 'share'))
            if not base.is_absolute():
                base = Path.home() / '.local' / 'share'
        return base / display_name()
    return Path(__file__).resolve().parents[2]


def resource_root():
    return Path(getattr(sys, '_MEIPASS', Path(__file__).resolve().parents[2]))


def display_name():
    if getattr(sys, 'frozen', False):
        import json
        variant=resource_root()/'app-variant.json'
        if variant.exists():
            return json.loads(variant.read_text(encoding='utf-8'))['name']
    return APP_NAME


def bundled_browser():
    if not getattr(sys, 'frozen', False):
        return None
    if sys.platform == 'darwin':
        browser = Path(sys.executable).parent.parent / 'Resources' / 'bundled-browser' / 'Camoufox.app' / 'Contents' / 'MacOS' / 'camoufox'
    else:
        browser = resource_root() / 'bundled-browser' / ('camoufox.exe' if sys.platform == 'win32' else 'camoufox-bin')
    return browser if browser.exists() else None


def worker_arguments(profile_id):
    command = ['worker', '--task', profile_id]
    return command if getattr(sys, 'frozen', False) else ['-m', 'app.main', *command]


def worker_executable():
    if getattr(sys, 'frozen', False) and sys.platform == 'darwin':
        return Path(sys.executable).parent.parent/'Helpers'/'Cita Worker.app'/'Contents'/'MacOS'/APP_NAME
    return Path(sys.executable)


def bundled_addons():
    if not getattr(sys,'frozen',False): return []
    root = (Path(sys.executable).parent.parent/'Resources' if sys.platform == 'darwin' else resource_root())/'default-addons'
    return sorted(p for p in root.glob('*') if (p/'manifest.json').exists())
