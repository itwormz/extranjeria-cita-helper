"""Verify public Sigstore build provenance against a fixed release policy."""
import json
import re
from functools import lru_cache

REPOSITORY = 'https://github.com/itwormz/extranjeria-cita-helper'
WORKFLOW = '.github/workflows/build.yml'
MAX_BUNDLE = 2 * 1024 * 1024


class AttestationError(ValueError):
    pass


def _unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise AttestationError('Duplicate attestation field')
        result[key] = value
    return result


@lru_cache(maxsize=1)
def _verifier():
    from importlib.resources import files, as_file
    from sigstore.verify import Verifier
    from sigstore.models import TrustedRoot
    # Never use production(offline=True): it trusts a mutable, unverified TUF cache.
    # Read only the production root shipped in our locked Sigstore distribution.
    resource = files('sigstore').joinpath('_store', 'https%3A%2F%2Ftuf-repo-cdn.sigstore.dev', 'trusted_root.json')
    with as_file(resource) as path:
        return Verifier(trusted_root=TrustedRoot.from_file(path))


def _verified_statement(raw, version):
    from sigstore.models import Bundle
    from sigstore.verify.policy import Identity, AllOf, OIDCSourceRepositoryURI, OIDCSourceRepositoryRef, OIDCRunnerEnvironment
    bundle = Bundle.from_json(raw)
    identity = f'{REPOSITORY}/{WORKFLOW}@refs/tags/v{version}'
    media_type, payload = _verifier().verify_dsse(
        bundle, AllOf([Identity(identity=identity, issuer='https://token.actions.githubusercontent.com'),
                       OIDCSourceRepositoryURI(REPOSITORY), OIDCSourceRepositoryRef(f'refs/tags/v{version}'),
                       OIDCRunnerEnvironment('github-hosted')]))
    if media_type != 'application/vnd.in-toto+json':
        raise AttestationError('Unsupported attestation payload')
    return bundle, json.loads(payload, object_pairs_hook=_unique_object)


def verify_attestation(raw, name, digest, version, *, expected_commit=None):
    """Return the certified source commit, or reject before trusting any claim."""
    try:
        if len(raw) > MAX_BUNDLE or not re.fullmatch(r'\d+\.\d+\.\d+', version):
            raise AttestationError('Invalid attestation input')
        if not re.fullmatch(r'[a-f0-9]{64}', digest):
            raise AttestationError('Invalid artifact digest')
        # Reject ambiguous JSON before the library parses the outer bundle.
        json.loads(raw, object_pairs_hook=_unique_object)
        bundle, statement = _verified_statement(raw, version)
        if statement['_type'] != 'https://in-toto.io/Statement/v1' or statement['predicateType'] != 'https://slsa.dev/provenance/v1':
            raise AttestationError('Unsupported provenance type')
        subjects = statement['subject']
        matches = [s for s in subjects if s.get('name') == name]
        if len(matches) != 1 or matches[0]['digest'] != {'sha256': digest}:
            raise AttestationError('Artifact does not match signed provenance')
        definition = statement['predicate']['buildDefinition']
        ref = f'refs/tags/v{version}'
        workflow = definition['externalParameters']['workflow']
        if definition['buildType'] != 'https://actions.github.io/buildtypes/workflow/v1' or workflow != {
                'repository': REPOSITORY, 'path': WORKFLOW, 'ref': ref}:
            raise AttestationError('Unexpected build workflow')
        github = definition['internalParameters']['github']
        if github['event_name'] != 'push' or github['runner_environment'] != 'github-hosted':
            raise AttestationError('Not a GitHub-hosted release build')
        sources = [d for d in definition['resolvedDependencies'] if d.get('uri') == f'git+{REPOSITORY}@{ref}']
        if len(sources) != 1:
            raise AttestationError('Missing or ambiguous source commit')
        commit = sources[0]['digest']['gitCommit']
        if not re.fullmatch(r'[a-f0-9]{40}', commit) or (expected_commit is not None and commit != expected_commit):
            raise AttestationError('Unexpected source commit')
        from sigstore.verify.policy import OIDCSourceRepositoryDigest
        OIDCSourceRepositoryDigest(commit).verify(bundle.signing_certificate)
        return commit
    except AttestationError:
        raise
    except Exception as exc:
        raise AttestationError('Package build provenance verification failed') from exc
