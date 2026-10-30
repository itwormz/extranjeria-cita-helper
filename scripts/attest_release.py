"""Attach build bundles to packages and verify them before release publication."""
import argparse
import hashlib
import os
from pathlib import Path
import re
import shutil
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from app.attestations import verify_attestation, MAX_BUNDLE


def packages(directory):
    return sorted(p for p in directory.iterdir() if p.suffix in {'.dmg', '.msi', '.AppImage', '.zip', '.zsync'})


def attach(directory, bundle):
    if not packages(directory):
        raise ValueError('No packages to attest')
    if bundle.stat().st_size > MAX_BUNDLE:
        raise ValueError('Attestation bundle too large')
    for package in packages(directory):
        shutil.copyfile(bundle, package.with_name(package.name + '.sigstore.json'))


def verified_attestations(assets, version, commit):
    if not isinstance(commit, str) or not re.fullmatch(r'[a-f0-9]{40}', commit):
        raise ValueError('Expected release commit is required')
    proofs = []
    for package in assets:
        if package.name == 'SHA256SUMS':
            continue
        proof = package.with_name(package.name + '.sigstore.json')
        with proof.open('rb') as stream:
            raw = stream.read(MAX_BUNDLE + 1)
        with package.open('rb') as stream:
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        verify_attestation(raw, package.name, digest, version, expected_commit=commit)
        proofs.append(proof)
    return proofs


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--bundle', type=Path, required=True)
    args = parser.parse_args()
    attach(args.directory, args.bundle)
