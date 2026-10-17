import hashlib
import json
import threading
from pathlib import Path
import pytest


def release(version='0.2.0'):
    base='https://github.com/itwormz/extranjeria-cita-helper/releases/download/v'+version+'/'
    name='extranjeria-cita-helper-'+version+'-windows-x64.msi'
    return {'tag_name':'v'+version,'assets':[{'name':name,'browser_download_url':base+name,'size':7},
            {'name':'SHA256SUMS','browser_download_url':base+'SHA256SUMS','size':200}]}


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


def test_download_verifies_checksum_and_never_keeps_corrupt_payload(tmp_path):
    from app.desktop.updates import select_release, download_release
    item=select_release(release(),'0.1.0','windows','x64')
    data=b'package';digest=hashlib.sha256(data).hexdigest()
    def fetch(url):
        import io
        return io.BytesIO((digest+'  '+item.name+'\n').encode() if url.endswith('SHA256SUMS') else data)
    output=download_release(item,tmp_path,threading.Event(),fetch=fetch)
    assert output.read_bytes()==data
    output.unlink()
    output.with_name(output.name+'.sha256').unlink()
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
    assert select_release(data,'0.1.0','windows','x64',portable=True).name.endswith('-portable.zip')


def test_linux_update_selects_appimage():
    from app.desktop.updates import select_release
    data=release();asset=data['assets'][0]
    asset['name']=asset['name'].replace('windows-x64.msi','linux-x64.AppImage')
    asset['browser_download_url']=asset['browser_download_url'].replace('windows-x64.msi','linux-x64.AppImage')
    assert select_release(data,'0.1.0','linux','x64').name.endswith('.AppImage')
