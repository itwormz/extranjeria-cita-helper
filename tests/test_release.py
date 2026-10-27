import tarfile
from pathlib import Path
import pytest
import os


@pytest.fixture(autouse=True)
def stable_publication_fixture(monkeypatch):
    # Publication behaviour is tested independently of the local dev version.
    from scripts import create_draft_release as release
    monkeypatch.setattr(release,'VERSION','0.2.1')


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


def test_tag_release_publishes_only_after_successful_asset_upload(tmp_path, monkeypatch):
    from types import SimpleNamespace
    from scripts import create_draft_release as release
    calls=[]
    monkeypatch.setenv('RELEASE_TAG', 'v'+release.VERSION)
    monkeypatch.setattr(release, 'verified_attestations', lambda *args:[])
    monkeypatch.setattr(release, 'verified_assets', lambda *args:[tmp_path/'package.msi'])
    def run(args, **kwargs):
        calls.append(args)
        return SimpleNamespace(returncode=1 if args[2]=='view' else 0)
    monkeypatch.setattr(release.subprocess, 'run', run)
    release.main(tmp_path, publish=True)
    assert calls[1][:3]==['gh','release','create'] and '--draft' in calls[1]
    assert calls[2][:3]==['gh','release','edit'] and '--draft=false' in calls[2]
    assert '## '+release.VERSION in (tmp_path/'release-notes.md').read_text()


def test_failed_upload_never_publishes_release(tmp_path, monkeypatch):
    import subprocess
    from types import SimpleNamespace
    from scripts import create_draft_release as release
    calls=[]
    monkeypatch.setenv('RELEASE_TAG', 'v'+release.VERSION)
    monkeypatch.setattr(release, 'verified_attestations', lambda *args:[])
    monkeypatch.setattr(release, 'verified_assets', lambda *args:[tmp_path/'package.msi'])
    def run(args, **kwargs):
        calls.append(args)
        if args[2]=='create': raise subprocess.CalledProcessError(1,args)
        return SimpleNamespace(returncode=1)
    monkeypatch.setattr(release.subprocess, 'run', run)
    with pytest.raises(subprocess.CalledProcessError): release.main(tmp_path,publish=True)
    assert not any(args[2]=='edit' for args in calls)


def test_existing_release_is_never_modified(tmp_path, monkeypatch):
    from types import SimpleNamespace
    from scripts import create_draft_release as release
    calls=[]
    monkeypatch.setenv('RELEASE_TAG', 'v'+release.VERSION)
    monkeypatch.setattr(release, 'verified_attestations', lambda *args:[])
    monkeypatch.setattr(release, 'verified_assets', lambda *args:[])
    monkeypatch.setattr(release.subprocess, 'run', lambda args, **kwargs: calls.append(args) or SimpleNamespace(returncode=0))
    with pytest.raises(SystemExit, match='left unchanged'): release.main(tmp_path,publish=True)
    assert len(calls)==1 and calls[0][2]=='view'


def test_manual_script_remains_draft_by_default(tmp_path, monkeypatch):
    from types import SimpleNamespace
    from scripts import create_draft_release as release
    calls=[]
    monkeypatch.setenv('RELEASE_TAG', 'v'+release.VERSION)
    monkeypatch.setattr(release, 'verified_attestations', lambda *args:[])
    monkeypatch.setattr(release, 'verified_assets', lambda *args:[])
    monkeypatch.setattr(release.subprocess, 'run', lambda args, **kwargs: calls.append(args) or SimpleNamespace(returncode=1 if args[2]=='view' else 0))
    release.main(tmp_path)
    assert len(calls)==2 and '--draft' in calls[-1]


def test_publication_workflow_is_gated_by_tag_and_all_platforms():
    import yaml
    workflow=yaml.load((Path(__file__).parents[1]/'.github/workflows/build.yml').read_text(),Loader=yaml.BaseLoader)
    job=workflow['jobs']['publish-release']
    assert job['if']=="github.event_name == 'push' && startsWith(github.ref, 'refs/tags/')"
    assert job['needs']=='build'
    assert job['permissions']['contents']=='write'
    targets=workflow['jobs']['build']['strategy']['matrix']['include']
    assert {(t['platform'],t['arch']) for t in targets}=={('macos','arm64'),('macos','x64'),('windows','x64'),('linux','x64')}
    assert any('--publish' in step.get('run','') for step in job['steps'])


def test_invalid_provenance_never_creates_or_publishes_release(tmp_path,monkeypatch):
    from scripts import create_draft_release as release
    monkeypatch.setenv('RELEASE_TAG','v'+release.VERSION)
    monkeypatch.setenv('GITHUB_SHA','b'*40)
    package=tmp_path/'app.msi';package.write_bytes(b'package')
    package.with_name(package.name+'.sigstore.json').write_text('{}')
    monkeypatch.setattr(release,'verified_assets',lambda *args:[package])
    calls=[];monkeypatch.setattr(release.subprocess,'run',lambda *args,**kwargs:calls.append(args))
    with pytest.raises(ValueError):release.main(tmp_path,publish=True)
    assert not calls


def test_development_version_never_starts_publication(tmp_path,monkeypatch):
    from scripts import create_draft_release as release
    monkeypatch.setattr(release,'VERSION','0.2.2.dev1')
    monkeypatch.setenv('RELEASE_TAG','v0.2.2.dev1')
    calls=[]
    monkeypatch.setattr(release.subprocess,'run',lambda *args,**kwargs:calls.append(args))
    with pytest.raises(ValueError,match='Release tag'):
        release.main(tmp_path,publish=True)
    assert not calls
