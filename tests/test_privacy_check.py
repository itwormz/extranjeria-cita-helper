import shutil
import pytest
from pathlib import Path
import subprocess
import sys

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/check_privacy.py'


@pytest.mark.skipif(shutil.which("git") is None, reason="Git is required for index privacy checks")
def test_staged_leak_is_rejected_without_printing_personal_values(tmp_path):
    subprocess.run(['git', 'init', '-q', str(tmp_path)], check=True)
    project = tmp_path
    (project / 'tasks.yml').write_text('tasks:\n  - applicant:\n      full_name: José Ejemplo\n      contact:\n        email: private@example.invalid\n', encoding="utf-8")
    (project / 'README.md').write_text('private@example.invalid JOSÉ EJEMPLO', encoding="utf-8")
    subprocess.run(['git', '-C', str(tmp_path), 'add', 'README.md'], check=True)
    result = subprocess.run([sys.executable, str(SCRIPT)], cwd=tmp_path, text=True, capture_output=True)
    assert result.returncode == 1
    assert 'contact_email' in result.stdout
    assert 'full_name' in result.stdout
    assert 'private@example.invalid' not in result.stdout
    (project / 'README.md').write_text('person@example.invalid', encoding="utf-8")
    subprocess.run(['git', '-C', str(tmp_path), 'add', 'README.md'], check=True)
    result = subprocess.run([sys.executable, str(SCRIPT)], cwd=tmp_path, text=True, capture_output=True)
    assert result.returncode == 0


def test_tree_check_excludes_local_preparation_but_checks_public_source(tmp_path):
    (tmp_path/'.local').mkdir()
    (tmp_path/'.local'/'review.log').write_text('local preparation')
    (tmp_path/'README.md').write_text('public source')
    result=subprocess.run([sys.executable,str(SCRIPT),'--tree',str(tmp_path)],text=True,capture_output=True)
    assert result.returncode==0
    assert '1 files, 0 findings' in result.stdout
