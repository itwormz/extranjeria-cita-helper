"""Build a self-contained macOS app without bundling user data or profiles."""
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile

NAME = 'Cita Extranjeria Helper'
ROOT = Path(__file__).resolve().parents[1]



def sign_browser(browser):
    """Sign nested Mach-O code before its containing browser bundle."""
    magic = {bytes.fromhex(value) for value in (
        "feedface", "cefaedfe", "feedfacf", "cffaedfe",
        "cafebabe", "bebafeca", "cafebabf", "bfbafeca")}
    info = plistlib.loads((browser / 'Contents' / 'Info.plist').read_bytes())
    executable = browser / 'Contents' / 'MacOS' / info['CFBundleExecutable']
    for path in sorted(browser.rglob("*")):
        if path == executable or path.is_symlink() or not path.is_file():
            continue
        with path.open("rb") as stream:
            is_code = stream.read(4) in magic
        if is_code:
            subprocess.run(['codesign', '--force', '--sign', '-', str(path)], check=True)
    subprocess.run(['codesign', '--force', '--sign', '-', str(browser)], check=True)

def build(*, experimental_no_fonts=False, include_fonts=False, output_directory=None):
    if sys.platform != 'darwin':
        from build_native import build_native
        return build_native(output_directory=output_directory)
    from PySide6.QtCore import QRectF
    from PySide6.QtGui import QImage, QPainter
    from PySide6.QtSvg import QSvgRenderer
    from camoufox.pkgman import camoufox_path
    from camoufox.addons import add_default_addons
    from camoufox.fpgen_model import ensure_fpgen_model, fpgen_data_dir, PIN, STAMP, PINNED_MTIME

    model=fpgen_data_dir()
    ensure_fpgen_model(model)
    model_names=[*PIN['file_sha256'],STAMP]
    model_args=[]
    for name in model_names:
        model_args.extend(['--add-data',f'{model/name}:fpgen/data'])
    addon_paths=[]
    add_default_addons(addon_paths)
    source = ROOT / 'app' / 'desktop' / 'assets'
    output = Path(output_directory) if output_directory else ROOT / 'data' / ('dist-experimental' if experimental_no_fonts else 'dist')
    work = ROOT / 'data' / ('build-app-experimental' if experimental_no_fonts else 'build-app')
    display_name = NAME + (' Experimental' if experimental_no_fonts else '')
    identifier = 'local.cita.extranjeria.helper' + ('.experimental' if experimental_no_fonts else '')
    browser = Path(camoufox_path()) / 'Camoufox.app'
    if not browser.is_dir():
        raise SystemExit('Fetch Camoufox before building: uv run python -m camoufox fetch')
    with tempfile.TemporaryDirectory(prefix='cita-app-') as temp:
        temp = Path(temp)
        iconset = temp / 'CitaHelper.iconset'
        iconset.mkdir()
        renderer = QSvgRenderer(str(source / 'icon.svg'))
        for size in (16, 32, 128, 256, 512):
            for scale in (1, 2):
                pixels = size * scale
                image = QImage(pixels, pixels, QImage.Format.Format_ARGB32)
                image.fill(0)
                painter = QPainter(image)
                renderer.render(painter, QRectF(0, 0, pixels, pixels))
                painter.end()
                image.save(str(iconset / f'icon_{size}x{size}{"@2x" if scale==2 else ""}.png'))
        icon = temp / 'CitaHelper.icns'
        subprocess.run(['iconutil', '-c', 'icns', str(iconset), '-o', str(icon)], check=True)
        # Explicit public resource allowlist: never add ROOT, tasks.yml or data/.
        subprocess.run([sys.executable, '-m', 'PyInstaller', '--noconfirm', '--windowed',
            '--name', NAME, '--osx-bundle-identifier', identifier,
            '--icon', str(icon), '--distpath', str(output), '--workpath', str(work),
            '--specpath', str(work), '--paths', str(ROOT),
            '--add-data', f'{source}:app/desktop/assets',
            '--add-data', f'{ROOT / "catalogs" / "snapshot-2026-08-12.json"}:catalogs',
            '--additional-hooks-dir', str(ROOT / 'scripts' / 'pyinstaller_hooks'),
            '--exclude-module', 'PIL', '--exclude-module', 'pygments.formatters.img',
            '--collect-data', 'language_tags',
            '--collect-all', 'sigstore',
            '--collect-all', 'sigstore_models',
            '--recursive-copy-metadata', 'sigstore',
            '--hidden-import', 'app.desktop.window', '--hidden-import', 'app.desktop.worker',
            *model_args, str(ROOT / 'scripts' / 'desktop_entry.py')], cwd=ROOT, check=True)
    bundle = output / (NAME + '.app')
    from build_native import trim_qt_translations
    trim_qt_translations(bundle / 'Contents' / 'Resources')
    destination = bundle / 'Contents' / 'Resources' / 'bundled-browser' / 'Camoufox.app'
    destination.parent.mkdir(parents=True, exist_ok=True)
    ignore = (lambda directory, names: ['fonts'] if Path(directory)==browser/'Contents'/'Resources' else []) if not include_fonts else None
    shutil.copytree(browser, destination, symlinks=True, dirs_exist_ok=True, ignore=ignore)
    from build_native import sanitize_certificate_policy
    sanitize_certificate_policy(destination)
    sign_browser(destination)
    if experimental_no_fonts:
        import json
        (bundle/'Contents'/'Resources'/'app-variant.json').write_text(json.dumps({'name':display_name,'experimental_no_fonts':True}))
        (bundle/'Contents'/'Frameworks'/'app-variant.json').symlink_to('../Resources/app-variant.json')
    # Preserve Camoufox's verified model and freshness stamp: it must never
    # download or write model files inside the signed, potentially read-only app.
    import os
    for name in model_names:
        target=bundle/'Contents'/'Frameworks'/'fpgen'/'data'/name
        os.utime(target,(PINNED_MTIME,PINNED_MTIME))
    for addon in addon_paths:
        addon=Path(addon)
        shutil.copytree(addon,bundle/'Contents'/'Resources'/'default-addons'/addon.name,symlinks=True,dirs_exist_ok=True)
    info_file = bundle / 'Contents' / 'Info.plist'
    info = plistlib.loads(info_file.read_bytes())
    from app.version import VERSION
    info.update(CFBundleShortVersionString=VERSION, CFBundleVersion=VERSION, CFBundleDisplayName=display_name, CFBundleName=display_name, NSHighResolutionCapable=True, LSMinimumSystemVersion='14.4')
    info_file.write_bytes(plistlib.dumps(info))
    # Give the worker its own background-only bundle identity. Its runtime and
    # public resources are shared with the containing app, never duplicated.
    helper = bundle/'Contents'/'Helpers'/'Cita Worker.app'
    helper_contents = helper/'Contents'
    (helper_contents/'MacOS').mkdir(parents=True, exist_ok=True)
    shutil.copy2(bundle/'Contents'/'MacOS'/NAME, helper_contents/'MacOS'/NAME)
    for directory in ('Frameworks', 'Resources'):
        link = helper_contents/directory
        if not link.is_symlink():
            link.symlink_to('../../../'+directory, target_is_directory=True)
    helper_info = dict(info, CFBundleIdentifier=identifier+'.worker',
                       CFBundleName='Cita Worker', CFBundleDisplayName='Cita Worker',
                       LSBackgroundOnly=True)
    helper_info.pop('CFBundleIconFile', None)
    (helper_contents/'Info.plist').write_bytes(plistlib.dumps(helper_info))
    subprocess.run(['codesign', '--force', '--sign', '-', str(helper)], check=True)
    subprocess.run(['codesign', '--force', '--sign', '-', str(bundle)], check=True)
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(bundle)], check=True)
    # COLLECT's standalone directory is only an intermediate for the .app.
    standalone = output / NAME
    if standalone.is_dir() and not standalone.is_symlink():
        shutil.rmtree(standalone)
    if experimental_no_fonts:
        final=ROOT/'data'/'dist'/(display_name+'.app')
        if final.exists():
            raise SystemExit(f'Existing experimental app left untouched. New build: {bundle}')
        final.parent.mkdir(parents=True,exist_ok=True)
        shutil.move(str(bundle),str(final))
        bundle=final
    print(bundle)
    return bundle


if __name__ == '__main__':
    import argparse
    parser=argparse.ArgumentParser()
    parser.add_argument('--experimental-no-fonts',action='store_true')
    parser.add_argument('--include-fonts',action='store_true',help='Include the complete bundled font collection')
    parser.add_argument('--output-dir',type=Path,help='Build in a separate output directory')
    args=parser.parse_args()
    build(experimental_no_fonts=args.experimental_no_fonts,include_fonts=args.include_fonts,output_directory=args.output_dir)
