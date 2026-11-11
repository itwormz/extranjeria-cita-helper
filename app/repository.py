"""Release repositories trusted by the updater: exact URLs only, new name first.

The project moves from extranjeria-cita-helper to santacita. Releases published
under either name are accepted, but every check compares against one exact
entry of this list: no prefixes, patterns or other repositories of the owner.
"""

REPOSITORIES = ('https://github.com/itwormz/santacita',
                'https://github.com/itwormz/extranjeria-cita-helper')
LEGACY_REPOSITORY = REPOSITORIES[1]


def trusted_repository(repository):
    if repository not in REPOSITORIES:
        raise ValueError('Untrusted release repository')
    return repository


def api_url(repository, path):
    """GitHub REST URL for one trusted repository, e.g. path='/releases/latest'."""
    owner_and_name = trusted_repository(repository).removeprefix('https://github.com/')
    return f'https://api.github.com/repos/{owner_and_name}{path}'
