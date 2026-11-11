import hashlib
import json
import threading
from pathlib import Path
from urllib.parse import urlsplit
import pytest


def release(version='0.2.0'):
    base='https://github.com/itwormz/extranjeria-cita-helper/releases/download/v'+version+'/'
    name='extranjeria-cita-helper-'+version+'-windows-x64.msi'
    return {'tag_name':'v'+version,'immutable':True,'assets':[{'name':name,'browser_download_url':base+name,'size':7},
            {'name':'SHA256SUMS','browser_download_url':base+'SHA256SUMS','size':200},
            {'name':name+'.sigstore.json','browser_download_url':base+name+'.sigstore.json','size':200}]}


def test_release_selects_native_installer_and_compares_versions():
    from app.desktop.updates import select_release
    result=select_release(release(),'0.1.0','windows','x64')
    assert result.version=='0.2.0'
    assert result.name.endswith('.msi')
    assert select_release(release('0.1.0'),'0.1.0','windows','x64') is None
    assert select_release(release('0.0.9'),'0.1.0','windows','x64') is None


@pytest.mark.parametrize('change',['external','wrong_project','traversal','oversized','duplicate'])
def test_release_rejects_untrusted_or_ambiguous_assets(change):
    from app.desktop.updates import select_release
    data=release()
    if change=='external': data['assets'][0]['browser_download_url']='https://evil.example/app.msi'
    if change=='wrong_project':data['assets'][0]['browser_download_url']=data['assets'][0]['browser_download_url'].replace('itwormz/','other/')
    if change=='traversal': data['tag_name']='v../../0.2.0'
    if change=='oversized': data['assets'][0]['size']=3*1024**3
    if change=='duplicate': data['assets'].append(dict(data['assets'][0]))
    with pytest.raises(ValueError):select_release(data,'0.1.0','windows','x64')


def test_download_verifies_checksum_and_never_keeps_corrupt_payload(tmp_path,monkeypatch):
    from app.desktop.updates import select_release, download_release
    item=select_release(release(),'0.1.0','windows','x64')
    monkeypatch.setattr('app.desktop.updates.verify_attestation',lambda *args,**kwargs:'b'*40)
    data=b'package';digest=hashlib.sha256(data).hexdigest()
    def fetch(url):
        import io
        if urlsplit(url).hostname == 'api.github.com':return io.BytesIO(json.dumps({'object':{'type':'commit','sha':'b'*40}}).encode())
        if url.endswith('.sigstore.json'):return io.BytesIO(b'{}')
        return io.BytesIO((digest+'  '+item.name+'\n').encode() if url.endswith('SHA256SUMS') else data)
    output=download_release(item,tmp_path,threading.Event(),fetch=fetch)
    assert output.read_bytes()==data
    output.unlink()
    output.with_name(output.name+'.sha256').unlink()
    output.with_name(output.name+'.provenance.json').unlink()
    def bad(url):
        import io
        return io.BytesIO((digest+'  '+item.name+'\n').encode() if url.endswith('SHA256SUMS') else b'bad')
    with pytest.raises(ValueError):download_release(item,tmp_path,threading.Event(),fetch=bad)
    assert not list(tmp_path.iterdir())


def test_cancelled_download_leaves_no_partial_file(tmp_path):
    from app.desktop.updates import select_release,download_release
    cancel=threading.Event();cancel.set()
    with pytest.raises(InterruptedError):
        download_release(select_release(release(),'0.1.0','windows','x64'),tmp_path,cancel)
    assert not list(tmp_path.iterdir())


def test_checksums_reject_duplicate_entries():
    from app.desktop.updates import checksum_for
    digest='a'*64
    with pytest.raises(ValueError):checksum_for((digest+'  app.msi\n')*2,'app.msi')


def test_platform_target_excludes_unsupported_architectures():
    from app.desktop.updates import release_target
    assert release_target('Darwin','arm64')==('macos','arm64')
    assert release_target('Windows','AMD64')==('windows','x64')
    assert release_target('Linux','x86_64')==('linux','x64')
    with pytest.raises(ValueError):release_target('Windows','ARM64')


def test_portable_release_never_selects_msi():
    from app.desktop.updates import select_release
    data=release()
    with pytest.raises(ValueError,match='asset'):
        select_release(data,'0.1.0','windows','x64',portable=True)
    asset=data['assets'][0]
    asset['name']=asset['name'].replace('.msi','-portable.zip')
    asset['browser_download_url']=asset['browser_download_url'].replace('.msi','-portable.zip')
    data['assets'][2]['name']=asset['name']+'.sigstore.json'
    data['assets'][2]['browser_download_url']=asset['browser_download_url']+'.sigstore.json'
    assert select_release(data,'0.1.0','windows','x64',portable=True).name.endswith('-portable.zip')


def test_linux_update_selects_appimage():
    from app.desktop.updates import select_release
    data=release();asset=data['assets'][0]
    asset['name']='Cita_Extranjeria_Helper-0.2.0-x86_64.AppImage'
    asset['browser_download_url']=asset['browser_download_url'].rsplit('/',1)[0]+'/'+asset['name']
    data['assets'][2]['name']=asset['name']+'.sigstore.json'
    data['assets'][2]['browser_download_url']=asset['browser_download_url']+'.sigstore.json'
    assert select_release(data,'0.1.0','linux','x64').name.endswith('.AppImage')

def test_missing_attestation_or_mutable_release_is_rejected():
    from app.desktop.updates import select_release
    for data in (dict(release(), immutable=False), dict(release(), assets=release()['assets'][:2])):
        with pytest.raises(ValueError):select_release(data,'0.1.0','windows','x64')


def test_replaced_package_and_checksum_cannot_bypass_provenance(tmp_path):
    import io
    from app.desktop.updates import select_release,download_release
    item=select_release(release(),'0.1.0','windows','x64')
    payload=b'malware';digest=hashlib.sha256(payload).hexdigest()
    def fetch(url):
        if url.endswith('SHA256SUMS'):return io.BytesIO((digest+'  '+item.name+'\n').encode())
        if urlsplit(url).hostname == 'api.github.com':return io.BytesIO(json.dumps({'object':{'type':'commit','sha':'b'*40}}).encode())
        if url.endswith('.sigstore.json'):return io.BytesIO(b'{}')
        return io.BytesIO(payload)
    with pytest.raises(ValueError):download_release(item,tmp_path,threading.Event(),fetch=fetch)
    assert not list(tmp_path.iterdir())


def test_install_reverifies_provenance_after_download(tmp_path,monkeypatch):
    from app.desktop.updates import verify_download
    package=tmp_path/'app.msi';package.write_bytes(b'package')
    digest=hashlib.sha256(b'package').hexdigest()
    package.with_name('app.msi.sha256').write_text(digest+'  app.msi\n')
    package.with_name('app.msi.provenance.json').write_text(json.dumps(dict(version='0.2.0',commit='b'*40,bundle={})))
    with pytest.raises(ValueError):verify_download(package)


def test_annotated_tag_resolution_is_bounded():
    import io
    from app.desktop.updates import release_commit
    calls=[]
    def fetch(url):
        calls.append(url)
        return io.BytesIO(json.dumps({'object':{'type':'tag','sha':'b'*40}}).encode())
    with pytest.raises(ValueError):release_commit('0.2.0',fetch)
    assert len(calls)==4


def test_dev_current_version_compares_with_stable_without_accepting_dev_releases():
    from app.desktop.updates import select_release
    assert select_release(release('0.2.1'),'0.2.2.dev1','windows','x64') is None
    assert select_release(release('0.2.2'),'0.2.2.dev1','windows','x64').version=='0.2.2'
    with pytest.raises(ValueError):
        select_release(release('0.2.2.dev2'),'0.2.2.dev1','windows','x64')


@pytest.mark.parametrize(("target", "arch", "portable", "name"), [
    ("windows", "x64", False, "SantaCita-0.2.0-windows-x64.msi"),
    ("windows", "x64", True, "SantaCita-0.2.0-windows-x64-portable.zip"),
    ("macos", "arm64", False, "SantaCita-0.2.0-macOS-Apple-Silicon.dmg"),
    ("macos", "x64", False, "SantaCita-0.2.0-macOS-Intel.dmg"),
    ("linux", "x64", False, "SantaCita-0.2.0-x86_64.AppImage"),
])
def test_renamed_release_assets(target, arch, portable, name):
    from app.desktop.updates import select_release

    data = release()
    old, sums, proof = data["assets"]
    data["assets"] += [
        {**old, "name": name, "browser_download_url": old["browser_download_url"].replace(old["name"], name)},
        {**proof, "name": name + ".sigstore.json",
         "browser_download_url": proof["browser_download_url"].replace(old["name"], name)},
    ]
    assert select_release(data, "0.1.0", target, arch, portable=portable).name == name


NEW_REPOSITORY = 'https://github.com/itwormz/santacita'
OLD_REPOSITORY = 'https://github.com/itwormz/extranjeria-cita-helper'


def repository_release(repository, version='0.2.0'):
    data = release(version)
    for item in data['assets']:
        item['browser_download_url'] = item['browser_download_url'].replace(OLD_REPOSITORY, repository)
    return data


@pytest.mark.parametrize('repository', [NEW_REPOSITORY, OLD_REPOSITORY])
def test_release_from_either_trusted_repository_is_accepted(repository):
    from app.desktop.updates import select_release
    item = select_release(repository_release(repository), '0.1.0', 'windows', 'x64', repository=repository)
    assert item.repository == repository
    assert item.url.startswith(repository + '/releases/download/v0.2.0/')


@pytest.mark.parametrize('url', [
    'https://github.com/itwormz/santacita-evil/releases/download/v0.2.0/',  # name suffix
    'https://github.com/itwormz/santacitaX/releases/download/v0.2.0/',
    'https://evil.github.com/itwormz/santacita/releases/download/v0.2.0/',  # subdomain
    'https://github.com.evil.example/itwormz/santacita/releases/download/v0.2.0/',
    'https://github.com/other/santacita/releases/download/v0.2.0/',  # other owner
    OLD_REPOSITORY + '/releases/download/v0.2.0/',  # the other trusted name, mixed into a new release
])
def test_release_rejects_package_outside_the_answering_repository(url):
    from app.desktop.updates import select_release
    data = repository_release(NEW_REPOSITORY)
    name = data['assets'][0]['name']
    data['assets'][0]['browser_download_url'] = url + name
    with pytest.raises(ValueError):
        select_release(data, '0.1.0', 'windows', 'x64', repository=NEW_REPOSITORY)


@pytest.mark.parametrize('repository', ['https://github.com/itwormz/santacita2', 'https://github.com/itwormz',
                                        'https://github.com/other/extranjeria-cita-helper'])
def test_release_rejects_untrusted_repository(repository):
    from app.desktop.updates import select_release
    with pytest.raises(ValueError):
        select_release(repository_release(repository), '0.1.0', 'windows', 'x64', repository=repository)


def test_latest_release_prefers_new_name_and_falls_back_only_on_404():
    import io
    from urllib.error import HTTPError
    from app.desktop.updates import latest_release
    calls = []

    def fetch(url, missing=()):
        calls.append(url)
        if any(name in url for name in missing):
            raise HTTPError(url, 404, 'Not Found', {}, None)
        return io.BytesIO(json.dumps({'tag_name': 'v0.2.0'}).encode())

    assert latest_release(fetch)[0] == NEW_REPOSITORY
    assert calls == ['https://api.github.com/repos/itwormz/santacita/releases/latest']
    calls.clear()
    assert latest_release(lambda url: fetch(url, missing=('/santacita/',)))[0] == OLD_REPOSITORY
    assert calls == ['https://api.github.com/repos/itwormz/santacita/releases/latest',
                     'https://api.github.com/repos/itwormz/extranjeria-cita-helper/releases/latest']

    def unavailable(url):
        raise HTTPError(url, 503, 'Unavailable', {}, None)
    with pytest.raises(HTTPError):
        latest_release(unavailable)


def test_release_commit_and_receipt_use_the_release_repository(tmp_path, monkeypatch):
    import io
    from app.desktop.updates import select_release, download_release, verify_download
    item = select_release(repository_release(NEW_REPOSITORY), '0.1.0', 'windows', 'x64', repository=NEW_REPOSITORY)
    checked = []
    monkeypatch.setattr('app.desktop.updates.verify_attestation',
                        lambda *args, **kwargs: checked.append(kwargs['repository']) or 'b' * 40)
    data = b'package'; digest = hashlib.sha256(data).hexdigest(); api = []

    def fetch(url):
        if urlsplit(url).hostname == 'api.github.com':
            api.append(url)
            return io.BytesIO(json.dumps({'object': {'type': 'commit', 'sha': 'b' * 40}}).encode())
        if url.endswith('.sigstore.json'): return io.BytesIO(b'{}')
        return io.BytesIO((digest + '  ' + item.name + '\n').encode() if url.endswith('SHA256SUMS') else data)
    output = download_release(item, tmp_path, threading.Event(), fetch=fetch)
    assert api == ['https://api.github.com/repos/itwormz/santacita/git/ref/tags/v0.2.0']
    receipt = json.loads(output.with_name(output.name + '.provenance.json').read_text())
    assert receipt['repository'] == NEW_REPOSITORY
    verify_download(output)
    assert checked == [NEW_REPOSITORY, NEW_REPOSITORY]


@pytest.mark.parametrize(('stored', 'expected'), [(None, OLD_REPOSITORY), (NEW_REPOSITORY, NEW_REPOSITORY)])
def test_downloaded_package_is_reverified_against_receipt_repository(tmp_path, monkeypatch, stored, expected):
    from app.desktop.updates import verify_download
    checked = []
    monkeypatch.setattr('app.desktop.updates.verify_attestation',
                        lambda *args, **kwargs: checked.append(kwargs['repository']) or 'b' * 40)
    package = tmp_path / 'app.msi'; package.write_bytes(b'package')
    package.with_name('app.msi.sha256').write_text(hashlib.sha256(b'package').hexdigest() + '  app.msi\n')
    receipt = dict(version='0.2.0', commit='b' * 40, bundle={})  # receipts from 0.3.7 have no repository
    if stored:
        receipt['repository'] = stored
    package.with_name('app.msi.provenance.json').write_text(json.dumps(receipt))
    verify_download(package)
    assert checked == [expected]


def test_downloaded_package_with_untrusted_receipt_repository_is_rejected(tmp_path):
    from app.desktop.updates import verify_download
    package = tmp_path / 'app.msi'; package.write_bytes(b'package')
    package.with_name('app.msi.sha256').write_text(hashlib.sha256(b'package').hexdigest() + '  app.msi\n')
    package.with_name('app.msi.provenance.json').write_text(json.dumps(dict(
        version='0.2.0', commit='b' * 40, bundle={}, repository='https://github.com/other/santacita')))
    with pytest.raises(ValueError):
        verify_download(package)
