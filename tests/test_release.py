import tarfile
from pathlib import Path
import pytest
import os


def test_tag_must_match_version():
    from scripts.package_release import validate_tag
    validate_tag('v0.1.0', '0.1.0')
    with pytest.raises(ValueError): validate_tag('v0.2.0', '0.1.0')
    with pytest.raises(ValueError): validate_tag('v0.1.0;echo', '0.1.0')


def test_linux_package_uses_appimage_builder(tmp_path,monkeypatch):
    from scripts.package_release import archive_bundle
    bundle=tmp_path/'bundle';bundle.mkdir();calls=[]
    def build(bundle,output,license_file,arch):
        calls.append(arch);output.write_bytes(b'appimage')
    monkeypatch.setattr('scripts.installers.build_appimage',build)
    archive=archive_bundle(bundle,tmp_path/'release','linux','x64','0.1.0')
    assert archive.suffix=='.AppImage' and calls==['x64']
    assert archive.with_name(archive.name+'.sha256').exists()


def test_windows_package_uses_msi_builder(tmp_path, monkeypatch):
    from scripts.package_release import archive_bundle
    bundle=tmp_path/'bundle'; bundle.mkdir()
    calls=[]
    def build(bundle,output,license_file,version,arch):
        calls.append((bundle,version,arch));output.write_bytes(b'msi')
    monkeypatch.setattr('scripts.installers.build_msi',build)
    archive=archive_bundle(bundle,tmp_path/'release','windows','x64','0.1.0')
    assert archive.suffix=='.msi'
    assert calls==[(bundle,'0.1.0','x64')]
    assert archive.with_name(archive.name+'.sha256').exists()


def test_draft_requires_every_platform_and_rejects_tampering(tmp_path):
    import hashlib
    from scripts.create_draft_release import verified_assets
    names = [('macos-arm64','dmg'),('macos-x64','dmg'),('linux-x64','AppImage'),('windows-x64','msi')]
    for target, extension in names:
        file = tmp_path/f'extranjeria-cita-helper-0.1.0-{target}.{extension}'
        file.write_bytes(b'archive')
        digest = hashlib.sha256(b'archive').hexdigest()
        file.with_name(file.name+'.sha256').write_text(f'{digest}  {file.name}\n', encoding='utf-8')
    assert len(verified_assets(tmp_path, '0.1.0')) == 5
    file.write_bytes(b'tampered')
    with pytest.raises(ValueError, match='Checksum mismatch'):
        verified_assets(tmp_path, '0.1.0')
