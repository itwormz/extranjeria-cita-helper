"""Native Windows/Linux packaging; never cross-compile or copy private data."""
from pathlib import Path
import os
import shutil
import subprocess
import sys
import tempfile

NAME = 'Cita Extranjeria Helper'
ROOT = Path(__file__).resolve().parents[1]


def build_native(*, output_directory=None):
    if sys.platform not in {'win32', 'linux'}:
        raise SystemExit('Native Windows/Linux builder requires that operating system.')
    from PySide6.QtCore import QRectF
    from PySide6.QtGui import QImage, QPainter
    from PySide6.QtSvg import QSvgRenderer
    from camoufox.pkgman import camoufox_path
    from camoufox.addons import add_default_addons
    from camoufox.fpgen_model import ensure_fpgen_model, fpgen_data_dir, PIN, STAMP, PINNED_MTIME
    from app.version import VERSION, native_version

    browser = Path(camoufox_path())
    executable = 'camoufox.exe' if sys.platform == 'win32' else 'camoufox-bin'
    if not (browser / executable).is_file():
        raise SystemExit('Fetch Camoufox before building.')
    output = Path(output_directory) if output_directory else ROOT / 'data' / 'dist'
    work = ROOT / 'data' / 'build-native'
    source = ROOT / 'app' / 'desktop' / 'assets'
    model = fpgen_data_dir()
    ensure_fpgen_model(model)
    model_names = [*PIN['file_sha256'], STAMP]
    model_args = []
    for name in model_names:
        model_args += ['--add-data', f'{model/name}:fpgen/data']
    addon_paths = []
    add_default_addons(addon_paths)
    with tempfile.TemporaryDirectory(prefix='cita-build-') as temporary:
        temporary = Path(temporary)
        image = QImage(256, 256, QImage.Format.Format_ARGB32)
        image.fill(0)
        painter = QPainter(image)
        QSvgRenderer(str(source / 'icon.svg')).render(painter, QRectF(0, 0, 256, 256))
        painter.end()
        icon = temporary / 'icon.png'
        image.save(str(icon))
        extra = []
        if sys.platform == 'win32':
            from PIL import Image
            converted = temporary / 'icon.ico'
            Image.open(icon).save(converted, sizes=[(16,16),(32,32),(48,48),(64,64),(128,128),(256,256)])
            icon = converted
            from PyInstaller.utils.win32.versioninfo import VSVersionInfo, FixedFileInfo, StringFileInfo, StringTable, StringStruct, VarFileInfo, VarStruct
            version_tuple = native_version(VERSION)
            info = VSVersionInfo(ffi=FixedFileInfo(filevers=version_tuple, prodvers=version_tuple),
                kids=[StringFileInfo([StringTable('040904B0', [StringStruct('FileDescription', NAME),
                    StringStruct('ProductName', NAME), StringStruct('FileVersion', VERSION), StringStruct('ProductVersion', VERSION)])]),
                    VarFileInfo([VarStruct('Translation', [1033, 1200])])])
            version_file = temporary / 'version.txt'
            version_file.write_text(str(info), encoding='utf-8')
            extra += ['--version-file', str(version_file)]
        subprocess.run([sys.executable, '-m', 'PyInstaller', '--noconfirm', '--onedir', '--windowed',
            '--name', NAME, '--icon', str(icon), '--distpath', str(output), '--workpath', str(work),
            '--specpath', str(work), '--paths', str(ROOT),
            '--add-data', f'{source}:app/desktop/assets',
            '--add-data', f'{ROOT/"catalogs"/"snapshot-2026-08-12.json"}:catalogs',
            '--additional-hooks-dir', str(ROOT / 'scripts' / 'pyinstaller_hooks'),
            '--exclude-module', 'PIL', '--exclude-module', 'pygments.formatters.img',
            '--collect-data', 'language_tags',
            '--collect-all', 'sigstore',
            '--collect-all', 'sigstore_models',
            '--recursive-copy-metadata', 'sigstore',
            '--hidden-import', 'app.desktop.window', '--hidden-import', 'app.desktop.worker',
            *model_args, *extra, str(ROOT / 'scripts' / 'desktop_entry.py')], cwd=ROOT, check=True)
    bundle = output / NAME
    resources = bundle / '_internal'
    trim_qt_translations(resources)
    # The application fixes the browser OS to the host: preserve all fonts
    # read by that OS, excluding only groups it never loads.
    copy_browser_runtime(browser, resources / 'bundled-browser', sys.platform)
    if sys.platform == 'win32':
        trim_legacy_windows_fonts(resources / 'bundled-browser' / 'fonts')
    deduplicate_fonts(resources / 'bundled-browser' / 'fonts')
    sanitize_certificate_policy(resources / 'bundled-browser')
    for addon in addon_paths:
        addon = Path(addon)
        shutil.copytree(addon, resources / 'default-addons' / addon.name, symlinks=True, dirs_exist_ok=True)
    for name in model_names:
        os.utime(resources / 'fpgen' / 'data' / name, (PINNED_MTIME, PINNED_MTIME))
    print(bundle)
    return bundle


def sanitize_certificate_policy(browser):
    """Do not ship certificate paths inherited from a developer's browser cache."""
    import json
    for path in Path(browser).rglob('policies.json'):
        document = json.loads(path.read_text(encoding='utf-8'))
        policies = document.get('policies', {})
        certificates = policies.get('Certificates', {})
        if 'Install' in certificates:
            certificates.pop('Install')
            if not certificates:
                policies.pop('Certificates')
            path.write_text(json.dumps(document, indent=2) + '\n', encoding='utf-8')


def trim_qt_translations(resources):
    """Keep base widget translations for the application's supported languages."""
    translations = Path(resources) / 'PySide6' / 'Qt' / 'translations'
    keep = {f'{module}_{language}.qm' for module in ('qt', 'qtbase')
            for language in ('en', 'es', 'ru')}
    for path in translations.glob('*.qm'):
        if path.name not in keep:
            path.unlink()


def copy_browser_runtime(source, destination, target):
    """Copy the browser intact, omitting only other operating systems' font groups."""
    import json
    source = Path(source)
    fonts = source / 'fonts'
    excluded = set()
    key = {'linux': 'lin', 'win32': 'win'}.get(target)
    try:
        metadata = json.loads((fonts / 'groups.json').read_text())
        groups = metadata['groups']
        keep = metadata['readBy'][key]
        valid = {'L', 'LM', 'LMW', 'LW', 'M', 'MW', 'W'}
        if (isinstance(groups, list) and isinstance(keep, list) and keep
                and all(isinstance(group, str) and group in valid for group in groups + keep)
                and set(keep) <= set(groups)):
            excluded = set(groups) - set(keep)
    except (OSError, ValueError, KeyError, TypeError):
        # A future or legacy layout must remain complete rather than lose fonts.
        pass
    def ignore(directory, names):
        return sorted(excluded.intersection(names)) if Path(directory) == fonts else []
    shutil.copytree(source, destination, symlinks=True, dirs_exist_ok=True, ignore=ignore)


def deduplicate_fonts(directory):
    """Drop byte-identical faces in legacy flat font bundles only.

    Grouped bundles encode OS visibility through directory membership and must
    stay untouched. Distinct faces, even with matching names, are preserved.
    """
    import hashlib
    directory=Path(directory)
    if not directory.is_dir() or (directory/'groups.json').exists():
        return 0
    seen={};saved=0
    for path in sorted(directory.iterdir()):
        if not path.is_file() or path.is_symlink() or path.suffix.lower() not in {'.ttf','.ttc','.otf'}:
            continue
        with path.open('rb') as stream:digest=hashlib.file_digest(stream,'sha256').hexdigest()
        if digest in seen:
            # Hash is only a shortlist; verify the complete contents before removal.
            if path.read_bytes()==seen[digest].read_bytes():
                saved+=path.stat().st_size;path.unlink()
        else:seen[digest]=path
    print(f'Exact duplicate font bytes omitted: {saved}')
    return saved


def font_families(path):
    from fontTools.ttLib import TTFont,TTCollection
    collection=TTCollection(path,lazy=True) if Path(path).suffix.lower() in {'.ttc','.otc'} else None
    fonts=collection.fonts if collection else [TTFont(path,lazy=True)]
    try:
        return {record.toUnicode().strip().casefold() for font in fonts
                for record in font['name'].names if record.nameID in {1,4,6,16}}
    finally:
        if collection:collection.close()
        else:fonts[0].close()


def trim_legacy_windows_fonts(directory, allowed=None, *, dry_run=False):
    """Keep every file providing a configured Windows face; retain unknowns.

    Do not modify font files or grouped layouts. Collections are kept whole,
    including all styles/faces, if any member belongs to Windows's allowlist.
    """
    import json
    import camoufox
    directory=Path(directory)
    if not directory.is_dir() or (directory/'groups.json').exists():return 0
    if allowed is None:
        allowed=json.loads((Path(camoufox.__file__).parent/'fonts.json').read_text(encoding='utf-8'))['win']
    if not isinstance(allowed,list) or not allowed or not all(isinstance(v,str) and v for v in allowed):
        raise ValueError('Windows font allowlist is invalid')
    wanted={name.strip().casefold() for name in allowed};remove=[];matched=set()
    for path in sorted(directory.iterdir()):
        if not path.is_file() or path.is_symlink() or path.suffix.lower() not in {'.ttf','.ttc','.otf','.otc'}:continue
        try:names=font_families(path)
        except Exception:continue  # Unknown faces stay intact.
        if not names:continue
        matches=names & wanted
        if matches:matched.update(matches)
        else:remove.append(path)
    if not matched:raise ValueError('No configured Windows font was found; refusing to trim')
    saved=sum(path.stat().st_size for path in remove)
    print(f'Windows legacy fonts: retain {len(matched)} configured names; omit {len(remove)} unrelated files ({saved} bytes)')
    if not dry_run:
        for path in remove:path.unlink()
    return saved
