"""Request CodeRabbit only after Greptile and required CI checks are complete."""
import argparse
import json
import re
import subprocess
from urllib.parse import quote


GREPTILE = 'greptile-apps'
TRANSITION_MARKER = '<!-- coderabbit-second-stage-requested -->'


class NotReady(RuntimeError):
    """Expected waiting state, unlike an API or configuration failure."""


def second_stage_started(comments, reviews):
    return any(((comment['user'].get('type') == 'Bot'
                 and comment['user']['login'] == 'github-actions[bot]')
                or (comment['user'].get('type') == 'User'
                    and comment.get('author_association') in {'OWNER', 'MEMBER', 'COLLABORATOR'}))
               and (comment.get('body') or '').startswith('@coderabbitai full review\n')
               and TRANSITION_MARKER in (comment.get('body') or '')
               for comment in comments) or any(
        review['user'].get('type') == 'Bot'
        and review['user']['login'].removesuffix('[bot]') == 'coderabbitai'
        and review['state'] not in {'PENDING', 'DISMISSED'} for review in reviews)


def is_greptile(login):
    return login.removesuffix('[bot]') == GREPTILE


def gh_json(arguments, *, allowed_codes=(0,)):
    result = subprocess.run(['gh', *arguments], capture_output=True, text=True, check=False)
    if result.returncode not in allowed_codes:
        operation = ' '.join(arguments[:2])
        message = result.stderr.strip() or 'GitHub request failed'
        raise RuntimeError(f'{operation}: {message}')
    return json.loads(result.stdout)


def review_threads(repository, number):
    owner, name = repository.split('/')
    query = '''query($owner:String!,$name:String!,$number:Int!,$cursor:String) {
      repository(owner:$owner,name:$name) {
        pullRequest(number:$number) {
          reviewThreads(first:100,after:$cursor) {
            nodes { isResolved comments(first:1) { nodes { author { __typename login } url } } }
            pageInfo { hasNextPage endCursor }
          }
        }
      }
    }'''
    threads = []
    cursor = None
    while True:
        arguments = ['api', 'graphql', '-f', f'query={query}', '-f', f'owner={owner}',
                     '-f', f'name={name}', '-F', f'number={number}']
        if cursor:
            arguments += ['-f', f'cursor={cursor}']
        result = gh_json(arguments)
        if result.get('errors'):
            raise RuntimeError('Could not read all review discussions')
        page = result['data']['repository']['pullRequest']['reviewThreads']
        threads.extend(page['nodes'])
        if not page['pageInfo']['hasNextPage']:
            return threads
        cursor = page['pageInfo']['endCursor']


def required_check_names(repository, number, base):
    # The branch summary needs contents:read; branchProtectionRule GraphQL
    # requires administration access unavailable to the normal GITHUB_TOKEN.
    branch = gh_json(['api', f'repos/{repository}/branches/{quote(base, safe="")}'])
    protection = branch.get('protection')
    if branch['protected'] and not isinstance(protection, dict):
        raise RuntimeError('The protected branch protection summary is unavailable.')
    required = (protection or {}).get('required_status_checks') or {}
    names = set(required.get('contexts', ()))
    names.update(check['context'] for check in required.get('checks', ()))
    # Rulesets can add checks beyond the older branch-protection rule.
    rules = gh_json(['api', '--paginate', '--slurp',
                     f'repos/{repository}/rules/branches/{quote(base, safe="")}'])
    for page in rules:
        for entry in page:
            if entry['type'] == 'required_status_checks':
                names.update(check['context'] for check in entry['parameters']['required_status_checks'])
    return names


def summary_commit(comments, repository):
    summaries = [comment for comment in comments
                 if comment['user'].get('type') == 'Bot' and is_greptile(comment['user']['login'])
                 and '<!-- greptile_summary -->' in (comment.get('body') or '')]
    latest = max(summaries, key=lambda comment: (comment['updated_at'], comment['id']), default=None)
    if latest is None:
        return None
    # Match the footer's reviewed commit, not arbitrary links elsewhere in the summary.
    match = re.search(r'Last reviewed commit:\s*\[[^\n]*?\]\(https://github\.com/'
                      + re.escape(repository) + r'/commit/([0-9a-f]{40})\)', latest['body'])
    return match[1] if match else None


def blockers(pull_request, reviews, threads, checks, required_names, *, summary_head=None,
             second_stage=False):
    reasons = []
    if pull_request['state'] != 'OPEN' or pull_request['isDraft']:
        reasons.append('The pull request must be open and ready for review.')
    published = [review for review in reviews
                 if review['user'].get('type') == 'Bot' and is_greptile(review['user']['login'])
                 and review['state'] not in {'PENDING', 'DISMISSED'}]
    latest = max(published, key=lambda review: review['id'], default=None)
    summary_ready = (summary_head == pull_request['headRefOid'] and any(
        check['name'] == 'Greptile Review' and check['bucket'] == 'pass' for check in checks))
    if not second_stage and latest is not None and latest['state'] == 'CHANGES_REQUESTED':
        reasons.append('Greptile still requests changes.')
    if not second_stage and latest is None and not summary_ready:
        reasons.append('Greptile has not published a review.')
    elif not second_stage and latest is not None and latest['commit_id'] != pull_request['headRefOid'] and not summary_ready:
        reasons.append('Greptile must review the current commit after the latest changes.')
    for thread in threads:
        comments = thread['comments']['nodes']
        author = comments[0].get('author') if comments else None
        if not second_stage and not thread['isResolved'] and author and author.get('__typename') == 'Bot' and is_greptile(author['login']):
            reasons.append('Unresolved Greptile finding: ' + comments[0]['url'])
    reported = {check['name'] for check in checks}
    for missing in sorted(set(required_names) - reported):
        reasons.append('Required check has not reported: ' + missing)
    if not required_names or not checks:
        reasons.append('No required CI checks were reported; readiness cannot be confirmed.')
    for check in checks:
        if check['name'] in required_names and check['bucket'] != 'pass':
            reasons.append(f"Required check {check['name']} is {check['bucket']}.")
        if not second_stage and check['name'].lower().startswith('greptile') and check['bucket'] != 'pass':
            reasons.append(f"Greptile check is {check['bucket']}.")
        if check['name'].lower().startswith('coderabbit') and check['bucket'] == 'pending':
            reasons.append('CodeRabbit is already running.')
    return reasons


def request_review(repository, number, *, request=False, automatic=False):
    arguments = ['pr', 'view', str(number), '--repo', repository,
                 '--json', 'headRefOid,state,isDraft,baseRefName']
    pull_request = gh_json(arguments)
    pages = gh_json(['api', '--paginate', '--slurp',
                     f'repos/{repository}/pulls/{number}/reviews'])
    reviews = [review for page in pages for review in page]
    comment_pages = gh_json(['api', '--paginate', '--slurp',
                            f'repos/{repository}/issues/{number}/comments'])
    comments = [comment for page in comment_pages for comment in page]
    started = second_stage_started(comments, reviews)
    if automatic and started:
        print(f'PR #{number}: CodeRabbit stage already started; no automatic repeat.')
        return
    summary_head = summary_commit(comments, repository)
    check_arguments = ['pr', 'checks', str(number), '--repo', repository, '--json', 'name,bucket']
    checks = gh_json([*check_arguments, '--required'], allowed_codes=(0, 1, 8))
    all_checks = gh_json(check_arguments, allowed_codes=(0, 1, 8))
    checks += [check for check in all_checks
               if check['name'].lower().startswith(('greptile', 'coderabbit'))]
    reasons = blockers(pull_request, reviews, review_threads(repository, number), checks,
                       required_check_names(repository, number, pull_request['baseRefName']),
                       summary_head=summary_head, second_stage=started)
    if reasons:
        raise NotReady('\n'.join(reasons))
    # Recheck the head after reading reviews, discussions and CI to catch a push
    # while the gate was checking. The caller must not push during the review.
    final = gh_json(arguments)
    if final['headRefOid'] != pull_request['headRefOid']:
        raise NotReady('The PR changed while checking readiness. Run the gate again.')
    if final['state'] != 'OPEN' or final['isDraft']:
        raise NotReady('The PR is no longer open and ready for review.')
    if request:
        subprocess.run(['gh', 'pr', 'comment', str(number), '--repo', repository,
                        '--body', '@coderabbitai full review\n\n' + TRANSITION_MARKER], check=True)
        print('CodeRabbit review requested for ' + pull_request['headRefOid'])
    else:
        print('Ready for CodeRabbit: ' + pull_request['headRefOid'])


def request_ready_reviews(repository, base, *, request=False):
    # A sweep handles delayed/missed check events and findings resolved after CI.
    pages = gh_json(['api', '--paginate', '--slurp',
                     f'repos/{repository}/pulls?state=open&base={quote(base, safe="")}&per_page=100'])
    errors = []
    for page in pages:
        for pull_request in page:
            if pull_request['draft']:
                continue
            number = pull_request['number']
            try:
                request_review(repository, number, request=request, automatic=True)
            except NotReady as exc:
                print(f'PR #{number}: waiting: {exc}')
            except RuntimeError as exc:
                errors.append(f'PR #{number}: {exc}')
    if errors:
        raise RuntimeError('\n'.join(errors))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', required=True, help='GitHub owner/repository')
    target = parser.add_mutually_exclusive_group(required=True)
    target.add_argument('--pr', type=int)
    target.add_argument('--automatic', action='store_true', help='Check ready open PRs; add --request to post reviews')
    parser.add_argument('--base', default='main', help='Target branch for the automatic sweep')
    parser.add_argument('--request', action='store_true', help='Post the full-review command')
    args = parser.parse_args()
    if (args.pr is not None and args.pr < 1) or len(args.repo.split('/')) != 2:
        parser.error('Use a positive PR number and owner/repository')
    try:
        if args.automatic:
            request_ready_reviews(args.repo, args.base, request=args.request)
        else:
            request_review(args.repo, args.pr, request=args.request)
    except (RuntimeError, KeyError, ValueError) as exc:
        raise SystemExit(str(exc)) from None


if __name__ == '__main__':
    main()
