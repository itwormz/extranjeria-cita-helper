"""Request CodeRabbit only after Greptile and required CI checks are complete."""
import argparse
import json
import subprocess
from urllib.parse import quote


GREPTILE = 'greptile-apps'


def is_greptile(login):
    return login.removesuffix('[bot]') == GREPTILE


def gh_json(arguments, *, allowed_codes=(0,)):
    result = subprocess.run(['gh', *arguments], capture_output=True, text=True, check=False)
    if result.returncode not in allowed_codes:
        raise RuntimeError(result.stderr.strip() or 'GitHub request failed')
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
    owner, name = repository.split('/')
    query = """query($owner:String!,$name:String!,$number:Int!) {
      repository(owner:$owner,name:$name) { pullRequest(number:$number) {
        baseRef { branchProtectionRule { requiredStatusCheckContexts } }
      } }
    }"""
    result = gh_json(['api', 'graphql', '-f', f'query={query}', '-f', f'owner={owner}',
                      '-f', f'name={name}', '-F', f'number={number}'])
    rule = result['data']['repository']['pullRequest']['baseRef']['branchProtectionRule']
    names = set(rule['requiredStatusCheckContexts'] if rule else ())
    # Rulesets can add checks beyond the older branch-protection rule.
    rules = gh_json(['api', '--paginate', '--slurp',
                     f'repos/{repository}/rules/branches/{quote(base, safe="")}'])
    for page in rules:
        for entry in page:
            if entry['type'] == 'required_status_checks':
                names.update(check['context'] for check in entry['parameters']['required_status_checks'])
    return names


def blockers(pull_request, reviews, threads, checks, required_names):
    reasons = []
    if pull_request['state'] != 'OPEN' or pull_request['isDraft']:
        reasons.append('The pull request must be open and ready for review.')
    published = [review for review in reviews
                 if review['user'].get('type') == 'Bot' and is_greptile(review['user']['login'])
                 and review['state'] not in {'PENDING', 'DISMISSED'}]
    latest = max(published, key=lambda review: review['id'], default=None)
    if latest is None:
        reasons.append('Greptile has not published a review.')
    elif latest['commit_id'] != pull_request['headRefOid']:
        reasons.append('Greptile must review the current commit after the latest changes.')
    elif latest['state'] == 'CHANGES_REQUESTED':
        reasons.append('Greptile still requests changes.')
    for thread in threads:
        comments = thread['comments']['nodes']
        author = comments[0].get('author') if comments else None
        if not thread['isResolved'] and author and author.get('__typename') == 'Bot' and is_greptile(author['login']):
            reasons.append('Unresolved Greptile finding: ' + comments[0]['url'])
    reported = {check['name'] for check in checks}
    for missing in sorted(set(required_names) - reported):
        reasons.append('Required check has not reported: ' + missing)
    if not required_names or not checks:
        reasons.append('No required CI checks were reported; readiness cannot be confirmed.')
    for check in checks:
        if check['name'] in required_names and check['bucket'] != 'pass':
            reasons.append(f"Required check {check['name']} is {check['bucket']}.")
        if check['name'].lower().startswith('greptile') and check['bucket'] != 'pass':
            reasons.append(f"Greptile check is {check['bucket']}.")
        if check['name'].lower().startswith('coderabbit') and check['bucket'] == 'pending':
            reasons.append('CodeRabbit is already running.')
    return reasons


def request_review(repository, number, *, request=False):
    arguments = ['pr', 'view', str(number), '--repo', repository,
                 '--json', 'headRefOid,state,isDraft,baseRefName']
    pull_request = gh_json(arguments)
    pages = gh_json(['api', '--paginate', '--slurp',
                     f'repos/{repository}/pulls/{number}/reviews'])
    reviews = [review for page in pages for review in page]
    check_arguments = ['pr', 'checks', str(number), '--repo', repository, '--json', 'name,bucket']
    checks = gh_json([*check_arguments, '--required'], allowed_codes=(0, 1, 8))
    all_checks = gh_json(check_arguments, allowed_codes=(0, 1, 8))
    checks += [check for check in all_checks
               if check['name'].lower().startswith(('greptile', 'coderabbit'))]
    reasons = blockers(pull_request, reviews, review_threads(repository, number), checks,
                       required_check_names(repository, number, pull_request['baseRefName']))
    if reasons:
        raise RuntimeError('\n'.join(reasons))
    # Recheck the head after reading reviews, discussions and CI to catch a push
    # while the gate was checking. The caller must not push during the review.
    final = gh_json(arguments)
    if final['headRefOid'] != pull_request['headRefOid']:
        raise RuntimeError('The PR changed while checking readiness. Run the gate again.')
    if final['state'] != 'OPEN' or final['isDraft']:
        raise RuntimeError('The PR is no longer open and ready for review.')
    if request:
        subprocess.run(['gh', 'pr', 'comment', str(number), '--repo', repository,
                        '--body', '@coderabbitai full review'], check=True)
        print('CodeRabbit review requested for ' + pull_request['headRefOid'])
    else:
        print('Ready for CodeRabbit: ' + pull_request['headRefOid'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', required=True, help='GitHub owner/repository')
    parser.add_argument('--pr', required=True, type=int)
    parser.add_argument('--request', action='store_true', help='Post the full-review command')
    args = parser.parse_args()
    if args.pr < 1 or len(args.repo.split('/')) != 2:
        parser.error('Use a positive PR number and owner/repository')
    try:
        request_review(args.repo, args.pr, request=args.request)
    except (RuntimeError, KeyError, ValueError) as exc:
        raise SystemExit(str(exc)) from None


if __name__ == '__main__':
    main()
