"""Verify all native archives before creating a draft; never publish a release."""
import hashlib
import os
from pathlib import Path
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from app.version import VERSION
from scripts.package_release import validate_tag


def verified_assets(directory, version):
    expected = [f'extranjeria-cita-helper-{version}-{target}.{extension}' for target, extension in
                [('macos-arm64','dmg'), ('macos-x64','dmg'), ('linux-x64','AppImage'), ('windows-x64','msi')]]
    portable=f'extranjeria-cita-helper-{version}-windows-x64-portable.zip'
    if (directory/portable).is_file():expected.append(portable)
    sums = []
    archives = []
    for name in expected:
        archive = directory / name
        with archive.open('rb') as source:
            digest = hashlib.file_digest(source, 'sha256').hexdigest()
        line = f'{digest}  {name}\n'
        if archive.with_name(name+'.sha256').read_text(encoding='utf-8') != line:
            raise ValueError(f'Checksum mismatch: {name}')
        archives.append(archive)
        sums.append(line)
    checksums = directory / 'SHA256SUMS'
    checksums.write_text(''.join(sums), encoding='utf-8')
    return [*archives, checksums]


def main(directory):
    tag = os.environ['RELEASE_TAG']
    validate_tag(tag, VERSION)
    assets = verified_assets(directory, VERSION)
    # Existing releases are left untouched, including drafts from earlier runs.
    existing = subprocess.run(['gh','release','view',tag], capture_output=True)
    if existing.returncode == 0:
        raise SystemExit('Release already exists; it was left unchanged.')
    notes = directory / 'release-notes.md'
    notes.write_text('Desktop packages for macOS (Apple Silicon / Intel), Linux x64 and Windows x64.\n\n'
                     'macOS: open the DMG and drag the app to Applications. Windows: run the MSI installer. Linux: make the AppImage executable and run it.\n'
                     'SHA256SUMS contains archive checksums.\n\n'
                     'macOS uses an ad-hoc signature; Windows packages do not have an Authenticode signature.\n', encoding='utf-8')
    subprocess.run(['gh','release','create',tag,*map(str,assets),'--verify-tag','--draft',
                    '--title',f'Cita Extranjeria Helper {VERSION}','--notes-file',str(notes)], check=True)


if __name__ == '__main__':
    main(Path(sys.argv[1]))
