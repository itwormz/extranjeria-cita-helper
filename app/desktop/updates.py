"""Public GitHub release checks and verified native-package downloads."""
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import ssl
import certifi
import threading
import time
from urllib.error import HTTPError
from urllib.request import Request, urlopen

from PySide6.QtCore import QObject,QThread,Signal

REPOSITORY='https://github.com/itwormz/extranjeria-cita-helper'
API='https://api.github.com/repos/itwormz/extranjeria-cita-helper/releases/latest'
MAX_PACKAGE=2*1024**3


def release_target(system=None,machine=None):
    system=system or platform.system();machine=(machine or platform.machine()).lower()
    target={'Darwin':'macos','Windows':'windows','Linux':'linux'}.get(system)
    arch={'aarch64':'arm64','arm64':'arm64','amd64':'x64','x86_64':'x64'}.get(machine)
    if not target or not arch or (target!='macos' and arch!='x64'):
        raise ValueError('Unsupported update target')
    return target,arch


def version_tuple(value):
    if not isinstance(value,str) or not re.fullmatch(r'\d+\.\d+\.\d+',value):
        raise ValueError('Invalid release version')
    return tuple(map(int,value.split('.')))


@dataclass(frozen=True)
class Release:
    version:str
    name:str
    url:str
    sums_url:str
    size:int


def select_release(data,current,target,arch,*,portable=False):
    tag=data.get('tag_name','')
    if not isinstance(tag,str) or not tag.startswith('v'):
        raise ValueError('Invalid release tag')
    version=tag[1:];version_tuple(version)
    if data.get('draft') or data.get('prerelease'):
        raise ValueError('Not a stable release')
    if version_tuple(version)<=version_tuple(current):return None
    extension={'windows':'msi','macos':'dmg','linux':'AppImage'}.get(target)
    if not extension or (target,arch) not in {('windows','x64'),('linux','x64'),('macos','arm64'),('macos','x64')}:
        raise ValueError('Unsupported update target')
    name=f'extranjeria-cita-helper-{version}-{target}-{arch}.{extension}'
    if portable:
        if target!='windows':raise ValueError('Portable updates require Windows')
        name=f'extranjeria-cita-helper-{version}-windows-{arch}-portable.zip'
    base=REPOSITORY+'/releases/download/'+tag+'/'
    def asset(name):
        found=[a for a in data.get('assets',[]) if a.get('name')==name]
        if len(found)!=1 or found[0].get('browser_download_url')!=base+name:
            raise ValueError('Missing or untrusted release asset')
        return found[0]
    package=asset(name);sums=asset('SHA256SUMS');size=package.get('size')
    if not isinstance(size,int) or isinstance(size,bool) or not 0<size<=MAX_PACKAGE:
        raise ValueError('Invalid package size')
    return Release(version,name,package['browser_download_url'],sums['browser_download_url'],size)


def fetch(url):
    return urlopen(Request(url,headers={'User-Agent':'Extranjeria-Cita-Helper','Accept':'application/vnd.github+json' if url==API else 'application/octet-stream'}),timeout=10,context=ssl.create_default_context(cafile=certifi.where()))


def checksum_for(data,name):
    text=data.decode('ascii') if isinstance(data,bytes) else data
    candidates=[]
    for line in text.splitlines():
        match=re.fullmatch(r'([a-fA-F0-9]{64})  (\S+)',line)
        if match and match[2]==name:candidates.append(match[1].lower())
    if len(candidates)!=1:raise ValueError('Missing or ambiguous checksum')
    return candidates[0]


def download_release(release,directory,cancel,*,fetch=fetch,progress=lambda value:None):
    if cancel.is_set():raise InterruptedError('Download cancelled')
    with fetch(release.sums_url) as response:
        sums=response.read(16385)
    if len(sums)>16384:raise ValueError('Checksum file too large')
    digest=checksum_for(sums,release.name)
    directory=Path(directory);directory.mkdir(parents=True,exist_ok=True)
    destination=directory/release.name;partial=directory/(release.name+'.part')
    actual=hashlib.sha256();count=0
    try:
        with fetch(release.url) as response,partial.open('wb') as output:
            while True:
                if cancel.is_set():raise InterruptedError('Download cancelled')
                chunk=response.read(1024*1024)
                if not chunk:break
                count+=len(chunk)
                if count>release.size or count>MAX_PACKAGE:raise ValueError('Package is too large')
                actual.update(chunk);output.write(chunk);progress(int(count*100/release.size))
        if count!=release.size or actual.hexdigest()!=digest:
            raise ValueError('Package checksum mismatch')
        os.replace(partial,destination)
        destination.with_name(destination.name+'.sha256').write_text(digest+'  '+destination.name+'\n')
        return destination
    finally:
        partial.unlink(missing_ok=True)


def verify_download(path):
    path=Path(path)
    digest=checksum_for(path.with_name(path.name+'.sha256').read_bytes(),path.name)
    with path.open('rb') as stream:
        if hashlib.file_digest(stream,'sha256').hexdigest()!=digest:
            raise ValueError('Downloaded package changed')
    return path


class UpdateJob(QThread):
    result=Signal(object)
    failed=Signal(str)
    progress=Signal(int)
    def __init__(self,operation,parent=None):
        super().__init__(parent);self.operation=operation;self.cancelled=threading.Event()
    def run(self):
        try:self.result.emit(self.operation(self))
        except InterruptedError:self.failed.emit('cancelled')
        except HTTPError as exc:self.failed.emit('no_release' if exc.code==404 else 'network')
        except Exception:self.failed.emit('failed')


class UpdateService(QObject):
    checked=Signal(object)
    downloaded=Signal(object)
    verified=Signal(object)
    failed=Signal(str)
    progress=Signal(int)
    busy_changed=Signal(bool)
    def __init__(self,directory,current,parent=None):
        super().__init__(parent);self.directory=Path(directory);self.current=current;self.job=None
    @property
    def busy(self):return self.job is not None
    def due(self):
        try:
            timestamp=json.loads((self.directory/'check.json').read_text())['checked_at']
            return not 0<=time.time()-float(timestamp)<86400
        except (OSError,ValueError,KeyError,TypeError):return True
    def _start(self,operation,signal):
        if self.busy:return False
        job=UpdateJob(operation,self);self.job=job
        job.result.connect(signal.emit);job.failed.connect(self.failed.emit);job.progress.connect(self.progress.emit)
        def finish():
            self.job=None;self.busy_changed.emit(False);job.deleteLater()
        job.finished.connect(finish);self.busy_changed.emit(True);job.start();return True
    def check(self):
        def operation(job):
            self.directory.mkdir(parents=True,exist_ok=True)
            (self.directory/'check.json').write_text(json.dumps({'checked_at':time.time()}))
            with fetch(API) as response:
                data=response.read(1024*1024+1)
            if len(data)>1024*1024:raise ValueError('Release response too large')
            from .paths import portable_mode
            return select_release(json.loads(data),self.current,*release_target(),portable=portable_mode())
        return self._start(operation,self.checked)
    def download(self,release):
        return self._start(lambda job:download_release(release,self.directory,job.cancelled,progress=job.progress.emit),self.downloaded)
    def verify(self,path):return self._start(lambda job:verify_download(path),self.verified)
    def cancel(self):
        if self.job:self.job.cancelled.set()
