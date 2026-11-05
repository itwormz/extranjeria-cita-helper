from unittest.mock import Mock

import pytest

from scripts import request_coderabbit_review as gate


def ready_state():
    return (
        {'state': 'OPEN', 'isDraft': False, 'headRefOid': 'current', 'baseRefName': 'main'},
        [{'id': 1, 'user': {'login': 'greptile-apps[bot]', 'type': 'Bot'},
          'state': 'COMMENTED', 'commit_id': 'current'}],
        [],
        [{'name': 'Tests', 'bucket': 'pass'}],
    )


@pytest.mark.parametrize('failure', ['missing', 'stale', 'requested', 'thread', 'ci', 'no_ci', 'draft', 'closed'])
def test_review_gate_blocks_unfinished_first_stage(failure):
    pr, reviews, threads, checks = ready_state()
    if failure == 'missing':
        reviews.clear()
    elif failure == 'stale':
        reviews[0]['commit_id'] = 'previous'
    elif failure == 'requested':
        reviews[0]['state'] = 'CHANGES_REQUESTED'
    elif failure == 'thread':
        threads.append({'isResolved': False, 'comments': {'nodes': [
            {'author': {'login': 'greptile-apps', '__typename': 'Bot'}, 'url': 'https://example.test/finding'}]}})
    elif failure == 'ci':
        checks[0]['bucket'] = 'pending'
    elif failure == 'no_ci':
        checks.clear()
    elif failure == 'draft':
        pr['isDraft'] = True
    else:
        pr['state'] = 'MERGED'
    assert gate.blockers(pr, reviews, threads, checks, {'Tests'})


def test_review_gate_accepts_fresh_review_and_resolved_findings():
    pr, reviews, threads, checks = ready_state()
    threads.append({'isResolved': True, 'comments': {'nodes': [
        {'author': {'login': 'greptile-apps', '__typename': 'Bot'}, 'url': 'https://example.test/finding'}]}})
    assert gate.blockers(pr, reviews, threads, checks, {'Tests'}) == []


@pytest.mark.parametrize('head_changes', [False, True, 'closed', 'draft'])
def test_review_request_rechecks_head_before_posting(monkeypatch, head_changes):
    pr, reviews, threads, checks = ready_state()
    final_pr = dict(pr, headRefOid='new' if head_changes is True else 'current')
    if head_changes == 'closed':
        final_pr['state'] = 'CLOSED'
    elif head_changes == 'draft':
        final_pr['isDraft'] = True
    monkeypatch.setattr(gate, 'gh_json', Mock(side_effect=[pr, [reviews], checks, [], final_pr]))
    monkeypatch.setattr(gate, 'review_threads', lambda *_: threads)
    monkeypatch.setattr(gate, 'required_check_names', lambda *_: {'Tests'})
    post = Mock()
    monkeypatch.setattr(gate.subprocess, 'run', post)
    if head_changes:
        with pytest.raises(RuntimeError):
            gate.request_review('owner/repo', 31, request=True)
        post.assert_not_called()
    else:
        gate.request_review('owner/repo', 31, request=True)
        post.assert_called_once_with(
            ['gh', 'pr', 'comment', '31', '--repo', 'owner/repo',
             '--body', '@coderabbitai full review'], check=True)


def test_review_gate_rejects_required_check_that_has_not_reported():
    pr, reviews, threads, checks = ready_state()
    assert any('Analyze (python)' in reason for reason in
               gate.blockers(pr, reviews, threads, checks, {'Tests', 'Analyze (python)'}))


@pytest.mark.parametrize('name,bucket', [('Greptile', 'pending'), ('Greptile', 'fail'), ('CodeRabbit', 'pending')])
def test_review_gate_does_not_overlap_another_review(name, bucket):
    pr, reviews, threads, checks = ready_state()
    checks.append({'name': name, 'bucket': bucket})
    assert gate.blockers(pr, reviews, threads, checks, {'Tests'})


def test_similarly_named_user_cannot_satisfy_greptile_gate():
    pr, reviews, threads, checks = ready_state()
    reviews[0]['user'] = {'login': 'greptile-apps', 'type': 'User'}
    assert gate.blockers(pr, reviews, threads, checks, {'Tests'})
