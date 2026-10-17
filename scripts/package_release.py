"""Archive a verified native bundle and generate its SHA256 sidecar."""
import argparse
import hashlib
from pathlib import Path
import re
import subprocess


def validate_tag(tag, version):
    if not re.fullmatch(r'\d+\.\d+\.\d+', version) or tag != 'v' + version:
        raise ValueError('Release tag must exactly match the application version')


def archive_bundle(bundle, output, platform, arch, version, *, portable=False):
    validate_tag('v' + version, version)
    if platform not in {'macos', 'linux', 'windows'} or arch not in {'arm64', 'x64'}:
        raise ValueError('Unsupported release target')
    output.mkdir(parents=True, exist_ok=True)
    basename = f'extranjeria-cita-helper-{version}-{platform}-{arch}'
    if portable and platform != 'windows':
        raise ValueError('Portable ZIP is only supported on Windows')
    archive = output / (basename + ('-portable.zip' if portable else {'linux':'.AppImage','macos':'.dmg','windows':'.msi'}[platform]))
    license_file=Path(__file__).resolve().parents[1]/'LICENSE'
    if platform == 'macos':
        from scripts.installers import build_dmg
        build_dmg(bundle,archive,license_file)
    elif platform == 'linux':
        from scripts.installers import build_appimage
        build_appimage(bundle,archive,license_file,arch)
    elif portable:
        from scripts.installers import build_portable_zip
        build_portable_zip(bundle,archive,license_file)
    else:
        from scripts.installers import build_msi
        build_msi(bundle,archive,license_file,version,arch)
    with archive.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    archive.with_name(archive.name + '.sha256').write_text(f'{digest}  {archive.name}\n', encoding='utf-8')
    return archive


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--platform', required=True)
    parser.add_argument('--arch', required=True)
    parser.add_argument('--check-tag')
    parser.add_argument('--portable',action='store_true',help='Optional Windows ZIP with adjacent data directory')
    args = parser.parse_args()
    import sys
    root = Path(__file__).resolve().parents[1]
    sys.path.insert(0, str(root))
    from app.version import VERSION
    if args.check_tag:
        validate_tag(args.check_tag, VERSION)
    else:
        bundle = root / 'data/dist' / ('Cita Extranjeria Helper.app' if args.platform == 'macos' else 'Cita Extranjeria Helper')
        print(archive_bundle(bundle, root/'data/releases', args.platform, args.arch, VERSION,portable=args.portable))
