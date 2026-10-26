import tomllib
from pathlib import Path

from app.version import VERSION


def test_version_matches_project_metadata():
    pyproject = Path(__file__).resolve().parents[1] / "pyproject.toml"
    metadata = tomllib.loads(pyproject.read_text(encoding="utf-8"))
    assert VERSION == metadata["project"]["version"]


def test_development_build_has_numeric_native_metadata():
    from app.version import native_version, macos_build_version
    assert native_version('0.2.2.dev3')==(0,2,2,3)
    assert native_version('0.2.2')==(0,2,2,0)
    assert macos_build_version('0.2.2.dev3')=='0.2.2d3'
    assert macos_build_version('0.2.2')=='0.2.2'
