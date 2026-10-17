"""Install a hash-pinned official AppImage tool and its embedded runtime."""
import argparse,hashlib,os,subprocess,urllib.request
from pathlib import Path

URL='https://github.com/AppImage/appimagetool/releases/download/1.9.0/appimagetool-x86_64.AppImage'
SHA256='46fdd785094c7f6e545b61afcfb0f3d98d8eab243f644b4b17698c01d06083d1'


def prepare(directory):
    directory=Path(directory).resolve();directory.mkdir(parents=True,exist_ok=True)
    image=directory/'appimagetool.AppImage'
    if not image.exists():
        with urllib.request.urlopen(URL,timeout=120) as response:image.write_bytes(response.read())
    if hashlib.sha256(image.read_bytes()).hexdigest()!=SHA256:
        raise ValueError('AppImage build tool checksum mismatch')
    image.chmod(0o755)
    offset=int(subprocess.check_output([str(image),'--appimage-offset'],text=True).strip())
    runtime=directory/'runtime-x86_64';runtime.write_bytes(image.read_bytes()[:offset])
    subprocess.run([str(image),'--appimage-extract'],cwd=directory,check=True,stdout=subprocess.DEVNULL)
    tool=directory/'squashfs-root/AppRun'
    if os.environ.get('GITHUB_ENV'):
        with open(os.environ['GITHUB_ENV'],'a') as output:
            output.write(f'APPIMAGETOOL={tool}\nAPPIMAGE_RUNTIME={runtime}\n')
    print(f'APPIMAGETOOL={tool}\nAPPIMAGE_RUNTIME={runtime}')
    return tool,runtime


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('directory');args=parser.parse_args();prepare(args.directory)
