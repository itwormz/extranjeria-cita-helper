"""Verify all packages, upload a draft, then optionally publish the completed release."""
import argparse
import hashlib
import os
from pathlib import Path
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from app.version import VERSION
from scripts.package_release import validate_tag
from scripts.attest_release import verified_attestations


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


def changelog_notes(version):
    lines = (Path(__file__).resolve().parents[1] / 'CHANGELOG.md').read_text(encoding='utf-8').splitlines()
    for start, line in enumerate(lines):
        if line == '## ' + version or line.startswith('## ' + version + ' '):
            end = next((i for i in range(start + 1, len(lines)) if lines[i].startswith('## ')), len(lines))
            section = '\n'.join(lines[start:end]).strip()
            if not any(line.strip() for line in lines[start+1:end]):
                break
            return section
    raise ValueError(f'Missing changelog entry for {version}')


def main(directory, *, publish=False):
    tag = os.environ['RELEASE_TAG']
    validate_tag(tag, VERSION)
    assets = verified_assets(directory, VERSION)
    assets += verified_attestations(assets, VERSION, os.environ.get("GITHUB_SHA"))
    # Existing releases are left untouched, including drafts from earlier runs.
    existing = subprocess.run(['gh','release','view',tag], capture_output=True)
    if existing.returncode == 0:
        raise SystemExit('Release already exists; it was left unchanged.')
    notes = directory / 'release-notes.md'
    notes.write_text(changelog_notes(VERSION) + '\n\n---\n\nDesktop packages for macOS (Apple Silicon / Intel), Linux x64 and Windows x64.\n\n'
                     'macOS: open the DMG and drag the app to Applications. Windows: run the MSI installer. Linux: make the AppImage executable and run it.\n'
                     'SHA256SUMS contains archive checksums. Each package includes a .sigstore.json build attestation verified before publication.\n\n'
                     'macOS uses an ad-hoc signature; Windows packages do not have an Authenticode signature.\n', encoding='utf-8')
    subprocess.run(['gh','release','create',tag,*map(str,assets),'--verify-tag','--draft',
                    '--title',f'Cita Extranjeria Helper {VERSION}','--notes-file',str(notes)], check=True)
    if publish:
        # Keep incomplete uploads private: publish only after gh create succeeded.
        subprocess.run(['gh','release','edit',tag,'--draft=false','--latest'], check=True)


if __name__ == '__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('directory',type=Path)
    parser.add_argument('--publish',action='store_true')
    args=parser.parse_args()
    main(args.directory,publish=args.publish)
