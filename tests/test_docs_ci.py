import subprocess
from pathlib import Path

import pytest

from scripts import ci_scope


@pytest.mark.parametrize('path', ['README.md', 'README.ru.md', 'CHANGELOG.md', 'LICENSE',
                                  'docs/BUILD.md', 'docs/images/monitor.png', 'docs/images/flow.svg'])
def test_documentation_allowlist(path):
    assert ci_scope.documentation_path(path)


@pytest.mark.parametrize('path', ['app/main.py', 'pyproject.toml', 'uv.lock',
                                  '.github/workflows/tests.yml', '.github/release-version',
                                  'docs/example.py', 'docs/config.yml', 'AGENTS.md',
                                  'scripts/README.md', 'docs/../app/main.py'])
def test_unknown_and_executable_paths_require_full_ci(path):
    assert not ci_scope.documentation_path(path)


def commit(repo, name):
    subprocess.run(['git', '-C', str(repo), 'add', '.'], check=True, capture_output=True)
    subprocess.run(['git', '-C', str(repo), '-c', 'user.name=Test',
                    '-c', 'user.email=test@example.test', 'commit', '-m', name], check=True, capture_output=True)
    return subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()


@pytest.fixture
def repository(tmp_path):
    subprocess.run(['git', 'init', str(tmp_path)], check=True, capture_output=True)
    (tmp_path / 'README.md').write_text('hello')
    commit(tmp_path, 'base')
    return tmp_path


def test_pr_scope_includes_code_before_latest_docs_commit(repository):
    repo = repository
    base = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()
    (repo / 'app.py').write_text('pass')
    commit(repo, 'code')
    (repo / 'README.md').write_text('updated')
    head = commit(repo, 'docs')
    assert ci_scope.classify('pull_request', {'pull_request': {
        'base': {'sha': base}, 'head': {'sha': head}}}, repo) is False


def test_rename_from_code_to_document_is_not_docs_only(repository):
    repo = repository
    (repo / 'app.py').write_text('pass')
    base = commit(repo, 'code')
    (repo / 'app.py').rename(repo / 'README.extra.md')
    head = commit(repo, 'rename')
    assert ci_scope.classify('push', {'before': base, 'after': head}, repo) is False


def test_documentation_only_pr_and_push(repository):
    repo = repository
    base = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()
    (repo / 'README.md').write_text('updated')
    head = commit(repo, 'docs')
    assert ci_scope.classify('pull_request', {'pull_request': {
        'base': {'sha': base}, 'head': {'sha': head}}}, repo) is True
    assert ci_scope.classify('push', {'before': base, 'after': head}, repo) is True


@pytest.mark.parametrize('event,payload', [('workflow_dispatch', {}), ('schedule', {}),
    ('push', {'before': '0' * 40, 'after': 'a' * 40}), ('unknown', {})])
def test_manual_scheduled_and_unknown_scope_run_full_ci(repository, event, payload):
    assert ci_scope.classify(event, payload, repository) is False


def test_invalid_git_diff_is_an_error_not_docs_only(repository):
    with pytest.raises(subprocess.CalledProcessError):
        ci_scope.classify('push', {'before': 'a' * 40, 'after': 'b' * 40}, repository)


@pytest.mark.parametrize('text,broken', [
    ('[local](guide.md)', False),
    ('![image](images/example.png)', False),
    ('<img src="images/example.png">', False),
    ('<source srcset="images/example.png 1x, images/missing.png 2x">', True),
    ('[missing](missing.md)', True),
    ('[reference][guide]\n\n[guide]: missing.md', True),
    ('[external](https://example.test/missing)', False),
    ('[section](#section)', False),
    ('```sh\n[not a link](missing.md)\n```', False),
    ('`[not a link](missing.md)`', False),
    ('```sh\nunclosed', True),
    ('<details>\n<summary>Example</summary>', True),
])
def test_documentation_checks_real_links_and_markup(tmp_path, text, broken):
    from scripts.check_docs import check_document

    (tmp_path / 'guide.md').write_text('Guide')
    (tmp_path / 'images').mkdir()
    (tmp_path / 'images/example.png').write_bytes(b'synthetic')
    source = tmp_path / 'README.md'
    source.write_text(text)
    assert bool(check_document(source)) is broken


def test_workflows_keep_required_jobs_and_condition_expensive_steps():
    import yaml

    root = Path(__file__).parents[1]
    for filename, job in [('tests.yml', 'tests'), ('codeql.yml', 'analyze')]:
        workflow = yaml.safe_load((root / '.github/workflows' / filename).read_text())
        steps = workflow['jobs'][job]['steps']
        scope = next(step for step in steps if step.get('id') == 'scope')
        assert scope['run'] == 'python3 scripts/ci_scope.py'
        assert steps[0]['with']['fetch-depth'] == 0
        assert 'if' not in workflow['jobs'][job]
        for step in steps:
            if (step.get('name') in {'Install Linux GUI libraries', 'Install locked dependencies',
                                    'Check release metadata', 'Check source privacy',
                                    'Run unit and offscreen UI tests', 'Initialize CodeQL', 'Analyze'}):
                assert step['if'] == "steps.scope.outputs.docs_only != 'true'"
        if filename == 'tests.yml':
            assert any(step.get('run') == 'python3 scripts/check_docs.py' for step in steps)
            lightweight = next(step for step in steps
                               if step.get('name') == 'Check documentation-only source privacy')
            assert lightweight['if'] == "steps.scope.outputs.docs_only == 'true'"
            assert '--no-project --with PyYAML==6.0.3' in lightweight['run']


def test_docs_checker_scans_uppercase_markdown_allowed_by_scope(tmp_path, monkeypatch):
    from scripts import check_docs

    document = tmp_path / 'docs/GUIDE.MD'
    document.parent.mkdir()
    document.write_text('[missing](missing.md)\n```sh\nunclosed')
    assert ci_scope.documentation_path('docs/GUIDE.MD')
    monkeypatch.chdir(tmp_path)
    assert check_docs.main() == 1
