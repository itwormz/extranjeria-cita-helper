import copy
import json
from pathlib import Path
from types import SimpleNamespace

import pytest

from app import attestations as a

DIGEST = 'a' * 64
COMMIT = 'b' * 40


def statement():
    ref = 'refs/tags/v0.2.0'
    return {'_type': 'https://in-toto.io/Statement/v1', 'predicateType': 'https://slsa.dev/provenance/v1',
            'subject': [{'name': 'app.msi', 'digest': {'sha256': DIGEST}}],
            'predicate': {'buildDefinition': {'buildType': 'https://actions.github.io/buildtypes/workflow/v1',
                'externalParameters': {'workflow': {'repository': a.REPOSITORY, 'path': a.WORKFLOW, 'ref': ref}},
                'internalParameters': {'github': {'event_name': 'push', 'runner_environment': 'github-hosted'}},
                'resolvedDependencies': [{'uri': f'git+{a.REPOSITORY}@{ref}', 'digest': {'gitCommit': COMMIT}}]}}}


@pytest.mark.parametrize('mutation', ['digest', 'duplicate', 'workflow', 'repository', 'ref', 'runner', 'event', 'type', 'commit'])
def test_signed_policy_rejects_unexpected_provenance(monkeypatch, mutation):
    data = statement()
    definition = data['predicate']['buildDefinition']
    if mutation == 'digest': data['subject'][0]['digest']['sha256'] = 'c' * 64
    if mutation == 'duplicate': data['subject'].append(copy.deepcopy(data['subject'][0]))
    if mutation in {'workflow', 'repository', 'ref'}:
        definition['externalParameters']['workflow'][{'workflow': 'path', 'repository': 'repository', 'ref': 'ref'}[mutation]] = 'wrong'
    if mutation == 'runner': definition['internalParameters']['github']['runner_environment'] = 'self-hosted'
    if mutation == 'event': definition['internalParameters']['github']['event_name'] = 'pull_request'
    if mutation == 'type': data['predicateType'] = 'unknown'
    if mutation == 'commit': definition['resolvedDependencies'][0]['digest']['gitCommit'] = 'c' * 40
    monkeypatch.setattr(a, '_verified_statement', lambda *args: (SimpleNamespace(signing_certificate=None), data))
    with pytest.raises(a.AttestationError):
        a.verify_attestation('{}', 'app.msi', DIGEST, '0.2.0', expected_commit=COMMIT)


def test_real_public_bundle_signature_and_payload_tampering():
    from sigstore.models import Bundle
    from sigstore.verify.policy import Identity, AllOf, OIDCSourceRepositoryURI, OIDCSourceRepositoryRef, OIDCRunnerEnvironment
    from sigstore.errors import VerificationError
    raw = (Path(__file__).parent / 'fixtures' / 'github-cli-build.sigstore.json').read_bytes()
    policy = Identity(identity='https://github.com/cli/cli/.github/workflows/deployment.yml@refs/heads/trunk',
                      issuer='https://token.actions.githubusercontent.com')
    policy=AllOf([policy,OIDCSourceRepositoryURI('https://github.com/cli/cli'),
                  OIDCSourceRepositoryRef('refs/heads/trunk'),OIDCRunnerEnvironment('github-hosted')])
    media, payload = a._verifier().verify_dsse(Bundle.from_json(raw), policy)
    assert media == 'application/vnd.in-toto+json'
    assert json.loads(payload)['predicateType'] == 'https://slsa.dev/provenance/v1'
    data = json.loads(raw)
    import base64
    data['dsseEnvelope']['payload'] = base64.b64encode(payload.replace(b'cli/cli', b'bad/bad')).decode()
    with pytest.raises(VerificationError):
        a._verifier().verify_dsse(Bundle.from_json(json.dumps(data)), policy)
    with pytest.raises(a.AttestationError):
        a.verify_attestation(raw, 'app.msi', DIGEST, '0.2.0')


def test_duplicate_fields_and_unsigned_json_are_rejected():
    for raw in ('{"x":1,"x":2}', json.dumps(statement()), '{}', 'x' * (a.MAX_BUNDLE + 1)):
        with pytest.raises(a.AttestationError):
            a.verify_attestation(raw, 'app.msi', DIGEST, '0.2.0')


def test_shipped_roots_ignore_external_cache(monkeypatch,tmp_path):
    from sigstore.verify import Verifier
    monkeypatch.setenv('XDG_CACHE_HOME',str(tmp_path))
    (tmp_path/'trusted_root.json').write_text('{}')
    def forbidden(*args,**kwargs):raise AssertionError('Mutable TUF cache must not be consulted')
    monkeypatch.setattr(Verifier,'production',forbidden)
    a._verifier.cache_clear()
    test_real_public_bundle_signature_and_payload_tampering()
    assert list(tmp_path.iterdir())==[tmp_path/'trusted_root.json']


def test_signed_statement_commit_must_match_certificate(monkeypatch):
    import base64
    from sigstore.models import Bundle
    raw=(Path(__file__).parent/'fixtures/github-cli-build.sigstore.json').read_bytes()
    bundle=Bundle.from_json(raw)
    actual=json.loads(base64.b64decode(json.loads(raw)['dsseEnvelope']['payload']))
    commit=actual['predicate']['buildDefinition']['resolvedDependencies'][0]['digest']['gitCommit']
    data=statement();data['predicate']['buildDefinition']['resolvedDependencies'][0]['digest']['gitCommit']=commit
    monkeypatch.setattr(a,'_verified_statement',lambda *args:(bundle,data))
    assert a.verify_attestation('{}','app.msi',DIGEST,'0.2.0',expected_commit=commit)==commit
    data['predicate']['buildDefinition']['resolvedDependencies'][0]['digest']['gitCommit']='c'*40
    with pytest.raises(a.AttestationError):a.verify_attestation('{}','app.msi',DIGEST,'0.2.0',expected_commit='c'*40)
