from pathlib import Path
import xml.etree.ElementTree as ET
import pytest
import os


def test_msi_contains_entire_bundle_stable_identity_and_per_user_shortcut(tmp_path):
    from scripts.installers import write_wix_source
    bundle=tmp_path/'Cita Extranjeria Helper'
    (bundle/'_internal').mkdir(parents=True)
    (bundle/'Cita Extranjeria Helper.exe').write_bytes(b'app')
    (bundle/'_internal'/'resource.txt').write_bytes(b'resource')
    source=write_wix_source(bundle,tmp_path/'package.wxs','0.1.0')
    tree=ET.parse(source); ns={'w':'http://wixtoolset.org/schemas/v4/wxs'}
    package=tree.find('w:Package',ns)
    assert package.get('Scope')=='perUser'
    upgrade=package.get('UpgradeCode')
    files=tree.findall('.//w:File',ns)
    assert {Path(f.get('Source')).name for f in files}=={'Cita Extranjeria Helper.exe','resource.txt'}
    assert tree.find('.//w:Shortcut',ns) is not None
    identity=tree.find('.//w:Shortcut/w:ShortcutProperty',ns)
    assert identity is not None
    assert identity.get('Key')=='System.AppUserModel.ID'
    assert identity.get('Value')=='itwormz.ExtranjeriaCitaHelper'
    assert tree.find('.//w:RemoveFolder',ns) is not None
    assert tree.find('.//w:MajorUpgrade',ns) is not None
    first_guids=[c.get('Guid') for c in tree.findall('.//w:Component',ns)]
    other=ET.parse(write_wix_source(bundle,tmp_path/'upgrade.wxs','0.2.0'))
    assert other.find('w:Package',ns).get('UpgradeCode')==upgrade
    assert [c.get('Guid') for c in other.findall('.//w:Component',ns)]==first_guids
    assert not tree.findall('.//w:RemoveFile',ns)


def test_msi_rejects_missing_executable_and_symlink_payload(tmp_path):
    from scripts.installers import write_wix_source
    bundle=tmp_path/'bundle';bundle.mkdir()
    with pytest.raises(ValueError,match='executable'):
        write_wix_source(bundle,tmp_path/'package.wxs','0.1.0')


def test_dmg_has_app_applications_link_and_license(tmp_path,monkeypatch):
    from scripts.installers import build_dmg
    bundle=tmp_path/'Helper.app';bundle.mkdir()
    license=tmp_path/'LICENSE';license.write_text('license')
    calls=[]
    def run(args,**kwargs):
        staging=Path(args[args.index('-srcfolder')+1])
        assert (staging/'Helper.app').is_dir()
        assert (staging/'Applications').is_symlink()
        assert (staging/'Applications').readlink()==Path('/Applications')
        assert (staging/'LICENSE.txt').read_text()=='license'
        calls.append(args)
        Path(args[-1]).write_bytes(b'dmg')
    monkeypatch.setattr('scripts.installers.subprocess.run',run)
    build_dmg(bundle,tmp_path/'helper.dmg',license)
    assert calls[0][:2]==['hdiutil','create']
    assert not list(tmp_path.glob('dmg-*'))


@pytest.mark.skipif(os.name == 'nt', reason='AppImage staging requires POSIX permissions and symlinks')
def test_appimage_contains_launcher_complete_bundle_and_license(tmp_path,monkeypatch):
    from scripts.installers import build_appimage
    bundle=tmp_path/'bundle';bundle.mkdir();(bundle/'Cita Extranjeria Helper').write_bytes(b'app')
    license=tmp_path/'LICENSE';license.write_text('notice')
    monkeypatch.setenv('APPIMAGETOOL','appimagetool');monkeypatch.setenv('APPIMAGE_RUNTIME','runtime')
    def run(args,**kwargs):
        staging=Path(args[-2])
        assert (staging/'AppRun').stat().st_mode & 0o111
        assert '\"$@\"' in (staging/'AppRun').read_text()
        assert (staging/'app/Cita Extranjeria Helper').read_bytes()==b'app'
        assert (staging/'LICENSE.txt').read_text()=='notice'
        assert (staging/'.DirIcon').is_symlink()
        assert kwargs['env']['ARCH']=='x86_64'
        assert '--runtime-file' in args
        assert args[args.index('-u')+1]=='gh-releases-zsync|itwormz|extranjeria-cita-helper|latest|Cita_Extranjeria_Helper-*-x86_64.AppImage.zsync'
        assert 'Name=Cita Extranjeria Helper' in (staging/'Cita_Extranjeria_Helper.desktop').read_text()
        Path(args[-1]).write_bytes(b'image')
        # zsyncmake writes basename.zsync in its working directory, not beside its input.
        (Path(kwargs.get('cwd',Path.cwd()))/(Path(args[-1]).name+'.zsync')).write_bytes(b'zsync metadata')
    monkeypatch.setattr('scripts.installers.subprocess.run',run)
    output=build_appimage(bundle,tmp_path/'helper.AppImage',license)
    assert output.stat().st_mode & 0o111
    assert output.with_name(output.name+'.zsync').is_file()


def test_portable_zip_uses_adjacent_data_without_shipping_user_files(tmp_path):
    import zipfile
    from scripts.installers import build_portable_zip
    bundle=tmp_path/'bundle';bundle.mkdir();(bundle/'helper.exe').write_bytes(b'app')
    license=tmp_path/'LICENSE';license.write_text('notice')
    archive=build_portable_zip(bundle,tmp_path/'portable.zip',license)
    with zipfile.ZipFile(archive) as package:
        assert package.read('Cita Extranjeria Helper/portable.flag')==b''
        assert 'Cita Extranjeria Helper/data/' in package.namelist()
        assert not any(name.endswith('tasks.yml') for name in package.namelist())


def test_portable_archive_rejects_existing_user_data(tmp_path):
    from scripts.installers import build_portable_zip
    bundle=tmp_path/'bundle';(bundle/'data').mkdir(parents=True)
    (bundle/'data/tasks.yml').write_text('private profile')
    with pytest.raises(ValueError,match='user data'):
        build_portable_zip(bundle,tmp_path/'portable.zip',tmp_path/'LICENSE')


def test_dev_msi_keeps_numeric_product_version(tmp_path):
    from scripts.installers import write_wix_source
    bundle=tmp_path/'bundle';bundle.mkdir()
    (bundle/'Cita Extranjeria Helper.exe').write_bytes(b'app')
    source=write_wix_source(bundle,tmp_path/'dev.wxs','0.2.2.dev3')
    ns={'w':'http://wixtoolset.org/schemas/v4/wxs'}
    assert ET.parse(source).find('w:Package',ns).get('Version')=='0.2.2'


@pytest.mark.skipif(os.name == 'nt', reason='AppImage staging requires POSIX permissions and symlinks')
@pytest.mark.parametrize('stale',[False,True])
def test_appimage_rejects_missing_zsync(tmp_path,monkeypatch,stale):
    from scripts.installers import build_appimage
    bundle=tmp_path/'bundle';bundle.mkdir()
    license=tmp_path/'LICENSE';license.write_text('notice')
    monkeypatch.setenv('APPIMAGETOOL','appimagetool')
    def run(args,**kwargs):Path(args[-1]).write_bytes(b'image')
    monkeypatch.setattr('scripts.installers.subprocess.run',run)
    if stale:(tmp_path/'helper.AppImage.zsync').write_bytes(b'old metadata')
    with pytest.raises(ValueError,match='zsync'):
        build_appimage(bundle,tmp_path/'helper.AppImage',license)
