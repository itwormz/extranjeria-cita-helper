"""Verify the final Linux release file offline, including browser and IPC."""
import argparse,json,os,subprocess,tempfile
from pathlib import Path


def verify(path):
    path=Path(path).resolve()
    if not path.is_file() or not os.access(path,os.X_OK):
        raise ValueError('AppImage must be an executable file')
    with tempfile.TemporaryDirectory(prefix='cita-appimage-smoke-') as directory:
        env=dict(os.environ,CITA_DATA_HOME=directory,QT_QPA_PLATFORM='offscreen')
        prefix=[str(path),'--appimage-extract-and-run']
        smoke=subprocess.run([*prefix,'--smoke-test'],env=env,capture_output=True,timeout=240)
        assert smoke.returncode==0,smoke.stderr.decode(errors='replace')
        rows=[json.loads(line) for line in smoke.stdout.decode().splitlines() if line.startswith('{')]
        assert any(row.get('ok') and row.get('browser') for row in rows)
        assert any(row.get('ok') and row.get('gui') for row in rows)
        ipc=subprocess.run([*prefix,'--ipc-smoke-test'],env=env,capture_output=True,timeout=180,
            input=b'{"command":"pause","paused":true}\n{"command":"stop"}\n')
        assert ipc.returncode==0,ipc.stderr.decode(errors='replace')
        rows=[json.loads(line) for line in ipc.stdout.decode().splitlines() if line.startswith('{')]
        assert rows[-1]=={'ok':True,'paused':True,'stopped':True}
    print('Final AppImage: offline browser, Widgets GUI and worker IPC OK')


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('image');verify(parser.parse_args().image)
