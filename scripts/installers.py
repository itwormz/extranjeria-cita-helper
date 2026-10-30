"""Native installer packaging; user data lives outside installation directories."""
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile
import uuid
import xml.etree.ElementTree as ET

NAMESPACE='http://wixtoolset.org/schemas/v4/wxs'
UPGRADE_CODE='20858c40-10fa-50d3-a1ed-7b8c5dc381c7'
IDENTITY=uuid.UUID(UPGRADE_CODE)
APP_NAME='Cita Extranjeria Helper'


def build_dmg(bundle, output, license_file):
    output=Path(output);output.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='dmg-',dir=output.parent) as temporary:
        staging=Path(temporary)
        shutil.copytree(bundle,staging/Path(bundle).name,symlinks=True)
        (staging/'Applications').symlink_to('/Applications')
        shutil.copyfile(license_file,staging/'LICENSE.txt')
        subprocess.run(['hdiutil','create','-volname',APP_NAME,'-srcfolder',str(staging),
                        '-format','UDZO','-ov',str(output)],check=True)
    return output


def write_wix_source(bundle, output, version, license_file=None):
    bundle=Path(bundle).resolve()
    if not (bundle/(APP_NAME+'.exe')).is_file():
        raise ValueError('Application executable is missing')
    if any(p.is_symlink() for p in bundle.rglob('*')):
        raise ValueError('MSI payload must not contain symlinks')
    from app.version import native_version
    installer_version='.'.join(map(str,native_version(version)[:3]))
    ET.register_namespace('',NAMESPACE)
    def element(parent,tag,**attributes):
        return ET.SubElement(parent,'{'+NAMESPACE+'}'+tag,attributes)
    def id_(name): return 'P'+hashlib.sha256(name.encode()).hexdigest()[:24]
    wix=ET.Element('{'+NAMESPACE+'}Wix')
    package=element(wix,'Package',Name=APP_NAME,Manufacturer='itwormz',Version=installer_version,
                    UpgradeCode=UPGRADE_CODE,Scope='perUser',InstallerVersion='500')
    element(package,'MajorUpgrade',DowngradeErrorMessage='A newer version is already installed.',Schedule='afterInstallInitialize')
    element(package,'MediaTemplate',EmbedCab='yes',CompressionLevel='high')
    element(package,'Property',Id='ARPCOMMENTS',Value='Appointment helper for Extranjería: fingerprinting and autorización de regreso')
    element(package,'Property',Id='ARPURLINFOABOUT',Value='https://github.com/itwormz/extranjeria-cita-helper')
    element(package,'Icon',Id='AppIcon',SourceFile=str(bundle/(APP_NAME+'.exe')))
    element(package,'Property',Id='ARPPRODUCTICON',Value='AppIcon')
    local=element(package,'StandardDirectory',Id='LocalAppDataFolder')
    programs=element(local,'Directory',Id='UserPrograms',Name='Programs')
    install=element(programs,'Directory',Id='INSTALLFOLDER',Name=APP_NAME)
    directories={Path('.'):install}
    feature=element(package,'Feature',Id='Application',Title=APP_NAME,Level='1')
    def directory(relative):
        if relative not in directories:
            directories[relative]=element(directory(relative.parent),'Directory',Id=id_('dir/'+relative.as_posix()),Name=relative.name)
        return directories[relative]
    files=[(p,p.relative_to(bundle)) for p in sorted(bundle.rglob('*')) if p.is_file()]
    if license_file:
        files.append((Path(license_file).resolve(),Path('LICENSE.txt')))
    for source,relative in files:
        name=relative.as_posix();cid=id_('component/'+name)
        component=element(directory(relative.parent),'Component',Id=cid,Guid=str(uuid.uuid5(IDENTITY,name)))
        element(component,'File',Id=id_('file/'+name),Source=str(source),Name=relative.name)
        element(component,'RegistryValue',Root='HKCU',Key='Software/itwormz/ExtranjeriaCitaHelper/Components',Name=cid,
                Type='integer',Value='1',KeyPath='yes')
        element(feature,'ComponentRef',Id=cid)
    menu=element(package,'StandardDirectory',Id='ProgramMenuFolder')
    menu_dir=element(menu,'Directory',Id='AppMenu',Name=APP_NAME)
    shortcut=element(menu_dir,'Component',Id='ApplicationShortcut',Guid=str(uuid.uuid5(IDENTITY,'shortcut')))
    launch=element(shortcut,'Shortcut',Id='StartMenuShortcut',Name=APP_NAME,Target='[INSTALLFOLDER]'+APP_NAME+'.exe',
            WorkingDirectory='INSTALLFOLDER',Icon='AppIcon')
    element(launch,'ShortcutProperty',Key='System.AppUserModel.ID',
            Value='itwormz.ExtranjeriaCitaHelper')
    element(shortcut,'RemoveFolder',Id='RemoveAppMenu',On='uninstall')
    element(shortcut,'RegistryValue',Root='HKCU',Key='Software/itwormz/ExtranjeriaCitaHelper',
            Name='installed',Type='integer',Value='1',KeyPath='yes')
    element(feature,'ComponentRef',Id='ApplicationShortcut')
    output=Path(output);output.parent.mkdir(parents=True,exist_ok=True)
    ET.indent(wix)
    ET.ElementTree(wix).write(output,encoding='utf-8',xml_declaration=True)
    return output


def build_msi(bundle, output, license_file, version, arch='x64'):
    output=Path(output);output.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='msi-',dir=output.parent) as temporary:
        source=write_wix_source(bundle,Path(temporary)/'package.wxs',
                                version,license_file)
        subprocess.run(['wix','build',str(source),'-arch',arch,'-o',str(output)],check=True)
    return output


def build_appimage(bundle, output, license_file, arch='x64'):
    """Package the complete native bundle as one executable AppImage."""
    import os
    if arch != 'x64':
        raise ValueError('AppImage target must be x64')
    tool=os.environ.get('APPIMAGETOOL') or shutil.which('appimagetool')
    if not tool:
        raise RuntimeError('Install appimagetool or set APPIMAGETOOL before packaging')
    if '/' in str(tool):tool=str(Path(tool).resolve())
    output=Path(output).resolve();output.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='appimage-',dir=output.parent) as temporary:
        staging=Path(temporary)/'Cita.AppDir';staging.mkdir()
        shutil.copytree(bundle,staging/'app',symlinks=True)
        shutil.copyfile(license_file,staging/'LICENSE.txt')
        icon=Path(__file__).resolve().parents[1]/'app/desktop/assets/icon.svg'
        shutil.copyfile(icon,staging/'cita-helper.svg')
        (staging/'.DirIcon').symlink_to('cita-helper.svg')
        (staging/'Cita_Extranjeria_Helper.desktop').write_text(
            '[Desktop Entry]\nType=Application\nName=Cita Extranjeria Helper\n'
            'Exec=AppRun\nIcon=cita-helper\nCategories=Utility;\nTerminal=false\n',encoding='utf-8')
        launcher=staging/'AppRun'
        launcher.write_text('#!/bin/sh\nHERE="$(dirname "$(readlink -f "$0")")"\n'
            'exec "$HERE/app/Cita Extranjeria Helper" "$@"\n',encoding='utf-8')
        launcher.chmod(0o755)
        environment=dict(os.environ,ARCH='x86_64',APPIMAGE_EXTRACT_AND_RUN='1')
        update='gh-releases-zsync|itwormz|extranjeria-cita-helper|latest|Cita_Extranjeria_Helper-*-x86_64.AppImage.zsync'
        args=[str(tool),'-u',update,'--no-appstream','--comp','zstd','--mksquashfs-opt','-processors','--mksquashfs-opt','2']
        if os.environ.get('APPIMAGE_RUNTIME'):
            args+=['--runtime-file',str(Path(os.environ['APPIMAGE_RUNTIME']).resolve())]
        zsync=output.with_name(output.name+'.zsync')
        zsync.unlink(missing_ok=True)
        subprocess.run([*args,str(staging),str(output)],cwd=output.parent,env=environment,check=True)
    if not zsync.is_file() or not zsync.stat().st_size:
        raise ValueError('AppImage zsync metadata is missing; install zsyncmake before packaging')
    output.chmod(0o755)
    return output


def build_portable_zip(bundle, output, license_file):
    """Optional Windows edition with private data next to the executable."""
    import zipfile
    private=Path(bundle)/'data'
    if (private.exists() and any(private.iterdir())) or (Path(bundle)/'tasks.yml').exists():
        raise ValueError('Portable payload contains user data')
    output=Path(output);output.parent.mkdir(parents=True,exist_ok=True)
    with zipfile.ZipFile(output,'w',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
        for path in sorted(Path(bundle).rglob('*')):
            if path.is_symlink():
                raise ValueError('Portable payload must not contain symlinks')
            if path.is_file():archive.write(path,'Cita Extranjeria Helper/'+path.relative_to(bundle).as_posix())
        archive.write(license_file,'Cita Extranjeria Helper/LICENSE.txt')
        archive.writestr('Cita Extranjeria Helper/portable.flag','')
        archive.writestr('Cita Extranjeria Helper/data/','')
    return output
