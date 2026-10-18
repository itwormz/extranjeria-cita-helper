import plistlib
from pathlib import Path
from unittest.mock import patch


def test_browser_signing_signs_nested_macho_before_bundle(tmp_path):
    from scripts.build_desktop_app import sign_browser
    browser = tmp_path / "Camoufox.app"
    binary = browser / "Contents" / "MacOS" / "libEGL.dylib"
    binary.parent.mkdir(parents=True)
    (browser / 'Contents' / 'Info.plist').write_bytes(plistlib.dumps({'CFBundleExecutable': 'camoufox'}))
    (binary.parent / 'camoufox').write_bytes(bytes.fromhex("cffaedfe") + b"main")
    binary.write_bytes(bytes.fromhex("cffaedfe") + b"binary")
    (binary.parent / "data.txt").write_text("resource")
    (binary.parent / "alias.dylib").symlink_to(binary.name)
    with patch("scripts.build_desktop_app.subprocess.run") as run:
        sign_browser(browser)
    signed = [Path(call.args[0][-1]) for call in run.call_args_list]
    assert signed == [binary, browser]
