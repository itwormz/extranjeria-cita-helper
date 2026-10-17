import tomllib
from pathlib import Path

from app.version import VERSION


def test_version_matches_project_metadata():
    pyproject = Path(__file__).resolve().parents[1] / "pyproject.toml"
    metadata = tomllib.loads(pyproject.read_text(encoding="utf-8"))
    assert VERSION == metadata["project"]["version"]
