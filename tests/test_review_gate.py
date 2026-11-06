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
    monkeypatch.setattr(gate, 'gh_json', Mock(side_effect=[pr, [reviews], [[]], checks, [], final_pr]))
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
             '--body', '@coderabbitai full review\n\n' + gate.TRANSITION_MARKER], check=True)


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


def summary_comment(commit='a' * 40, *, author_type='Bot', repository='owner/repo'):
    return {'id': 10, 'updated_at': '2026-11-05T17:00:00Z',
            'user': {'login': 'greptile-apps[bot]', 'type': author_type},
            'body': '<!-- greptile_summary -->\n<sub>Last reviewed commit: '
                    f'[fix](https://github.com/{repository}/commit/{commit})</sub>'}


def test_summary_update_can_complete_review_without_new_github_review():
    pr, reviews, threads, checks = ready_state()
    pr['headRefOid'] = 'a' * 40
    reviews[0]['commit_id'] = 'older'
    checks.append({'name': 'Greptile Review', 'bucket': 'pass'})
    summary = gate.summary_commit([summary_comment()], 'owner/repo')
    assert gate.blockers(pr, reviews, threads, checks, {'Tests'}, summary_head=summary) == []


@pytest.mark.parametrize('failure', ['user', 'repo', 'stale', 'marker', 'newer_summary', 'pending', 'no_check', 'requested', 'thread'])
def test_summary_fallback_does_not_bypass_review_guards(failure):
    pr, reviews, threads, checks = ready_state()
    pr['headRefOid'] = 'a' * 40
    reviews[0]['commit_id'] = 'older'
    comments = [summary_comment()]
    checks.append({'name': 'Greptile Review', 'bucket': 'pass'})
    if failure == 'user':
        comments[0]['user']['type'] = 'User'
    elif failure == 'repo':
        comments = [summary_comment(repository='other/repo')]
    elif failure == 'stale':
        comments = [summary_comment('b' * 40)]
    elif failure == 'marker':
        comments[0]['body'] = comments[0]['body'].replace('<!-- greptile_summary -->', '')
    elif failure == 'newer_summary':
        newer = summary_comment('b' * 40)
        newer.update(id=11, updated_at='2026-11-05T18:00:00Z')
        comments.append(newer)
    elif failure == 'pending':
        checks[-1]['bucket'] = 'pending'
    elif failure == 'no_check':
        checks.pop()
    elif failure == 'requested':
        reviews[0]['state'] = 'CHANGES_REQUESTED'
    else:
        threads.append({'isResolved': False, 'comments': {'nodes': [
            {'author': {'login': 'greptile-apps', '__typename': 'Bot'}, 'url': 'finding'}]}})
    summary = gate.summary_commit(comments, 'owner/repo')
    assert gate.blockers(pr, reviews, threads, checks, {'Tests'}, summary_head=summary)


def test_automatic_transition_posts_once_even_after_later_push(monkeypatch):
    pr, reviews, threads, checks = ready_state()
    comments = []
    posted = []

    def github(args, **kwargs):
        if args[:2] == ['pr', 'view']:
            return dict(pr)
        if args[-1].endswith('/reviews'):
            return [reviews]
        if args[-1].endswith('/comments'):
            return [comments]
        if args[:2] == ['pr', 'checks']:
            return list(checks) if '--required' in args else []
        raise AssertionError(args)

    def post(args, **kwargs):
        posted.append(args)
        comments.append({'user': {'type': 'Bot', 'login': 'github-actions[bot]'},
                         'body': args[-1]})

    monkeypatch.setattr(gate, 'gh_json', github)
    monkeypatch.setattr(gate, 'review_threads', lambda *_: threads)
    monkeypatch.setattr(gate, 'required_check_names', lambda *_: {'Tests'})
    monkeypatch.setattr(gate.subprocess, 'run', post)
    gate.request_review('owner/repo', 32, request=True, automatic=True)
    assert len(posted) == 1
    assert '@coderabbitai full review' in posted[0][-1]
    pr['headRefOid'] = 'later-coderabbit-fix'
    gate.request_review('owner/repo', 32, request=True, automatic=True)
    assert len(posted) == 1
    # A manual repeat checks CI, but does not demand another Greptile review.
    gate.request_review('owner/repo', 32, request=True)
    assert len(posted) == 2
    checks[0]['bucket'] = 'pending'
    with pytest.raises(gate.NotReady):
        gate.request_review('owner/repo', 32, request=True)
    assert len(posted) == 2


@pytest.mark.parametrize('failure', ['stale', 'thread', 'ci'])
def test_automatic_transition_keeps_first_stage_guards(monkeypatch, failure):
    pr, reviews, threads, checks = ready_state()
    if failure == 'stale':
        reviews[0]['commit_id'] = 'old'
    elif failure == 'thread':
        threads.append({'isResolved': False, 'comments': {'nodes': [
            {'author': {'login': 'greptile-apps', '__typename': 'Bot'}, 'url': 'finding'}]}})
    else:
        checks[0]['bucket'] = 'pending'
    monkeypatch.setattr(gate, 'gh_json', Mock(side_effect=[pr, [reviews], [[]], checks, []]))
    monkeypatch.setattr(gate, 'review_threads', lambda *_: threads)
    monkeypatch.setattr(gate, 'required_check_names', lambda *_: {'Tests'})
    post = Mock()
    monkeypatch.setattr(gate.subprocess, 'run', post)
    with pytest.raises(gate.NotReady):
        gate.request_review('owner/repo', 32, request=True, automatic=True)
    post.assert_not_called()


@pytest.mark.parametrize('author_type,login,expected', [
    ('Bot', 'github-actions[bot]', True),
    ('User', 'github-actions', False),
    ('Bot', 'unrelated[bot]', False),
])
def test_transition_marker_only_trusted_from_actions(author_type, login, expected):
    comments = [{'user': {'type': author_type, 'login': login},
                 'body': '@coderabbitai full review\n\n' + gate.TRANSITION_MARKER}]
    assert gate.second_stage_started(comments, []) is expected


def test_existing_coderabbit_review_prevents_another_automatic_first_review():
    review = {'user': {'type': 'Bot', 'login': 'coderabbitai[bot]'}, 'state': 'COMMENTED'}
    assert gate.second_stage_started([], [review])
    review['user']['type'] = 'User'
    assert not gate.second_stage_started([], [review])


def test_automatic_sweep_waits_for_unready_prs_but_checks_the_rest(monkeypatch):
    monkeypatch.setattr(gate, 'gh_json', Mock(return_value=[[{'number': 1, 'draft': False}],
                                                         [{'number': 2, 'draft': True},
                                                          {'number': 3, 'draft': False}]]))
    requests = Mock(side_effect=[gate.NotReady('CI pending'), None])
    monkeypatch.setattr(gate, 'request_review', requests)
    gate.request_ready_reviews('owner/repo', 'main')
    assert [call.args[1] for call in requests.call_args_list] == [1, 3]


def test_automatic_sweep_does_not_hide_api_errors(monkeypatch):
    monkeypatch.setattr(gate, 'gh_json', Mock(return_value=[[{'number': 1, 'draft': False}]]))
    monkeypatch.setattr(gate, 'request_review', Mock(side_effect=RuntimeError('API unavailable')))
    with pytest.raises(RuntimeError, match='API unavailable'):
        gate.request_ready_reviews('owner/repo', 'main')


def test_second_stage_still_requires_open_pr_ci_and_no_overlapping_review():
    pr, reviews, threads, checks = ready_state()
    reviews[0].update(commit_id='old', state='CHANGES_REQUESTED')
    checks.append({'name': 'Greptile Review', 'bucket': 'fail'})
    assert not gate.blockers(pr, reviews, threads, checks, {'Tests'}, second_stage=True)
    checks.append({'name': 'CodeRabbit', 'bucket': 'pending'})
    assert gate.blockers(pr, reviews, threads, checks, {'Tests'}, second_stage=True)
    checks.pop()
    pr['state'] = 'CLOSED'
    assert gate.blockers(pr, reviews, threads, checks, {'Tests'}, second_stage=True)


def test_coderabbit_pending_review_is_not_a_completed_transition():
    reviews = [{'user': {'type': 'Bot', 'login': 'coderabbitai[bot]'}, 'state': 'PENDING'}]
    assert not gate.second_stage_started([], reviews)


@pytest.mark.parametrize('association,expected', [('OWNER', True), ('MEMBER', True),
                                                ('COLLABORATOR', True), ('CONTRIBUTOR', False),
                                                ('NONE', False)])
def test_authorized_local_request_is_also_a_transition(association, expected):
    comment = {'user': {'type': 'User', 'login': 'maintainer'},
               'author_association': association,
               'body': '@coderabbitai full review\n\n' + gate.TRANSITION_MARKER}
    assert gate.second_stage_started([comment], []) is expected


def test_quoted_transition_marker_does_not_bypass_first_stage():
    comment = {'user': {'type': 'Bot', 'login': 'github-actions[bot]'},
               'body': 'Example command:\n@coderabbitai full review\n' + gate.TRANSITION_MARKER}
    assert not gate.second_stage_started([comment], [])
