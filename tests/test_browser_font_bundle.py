import json
from pathlib import Path
import pytest
from scripts.build_native import copy_browser_runtime


@pytest.mark.parametrize('target,keep', [('linux', {'L','LM','LW','LMW'}),
                                         ('win32', {'W','MW','LW','LMW'})])
def test_copy_preserves_every_font_read_by_target(tmp_path, target, keep):
    source = tmp_path / 'source'
    fonts = source / 'fonts'
    groups = ['L','LM','LMW','LW','M','MW','W']
    for group in groups:
        directory = fonts / group
        directory.mkdir(parents=True)
        (directory / 'example.ttf').write_bytes(group.encode())
    metadata = {'groups': groups, 'readBy': {'lin':['L','LM','LW','LMW'], 'win':['W','MW','LW','LMW']}}
    (fonts / 'groups.json').write_text(json.dumps(metadata))
    (source / 'LICENSE').write_text('license')
    destination = tmp_path / 'destination'
    copy_browser_runtime(source, destination, target)
    assert {p.name for p in (destination / 'fonts').iterdir() if p.is_dir()} == keep
    for group in keep:
        assert (destination / 'fonts' / group / 'example.ttf').read_bytes() == group.encode()
    assert (destination / 'fonts/groups.json').read_text() == (fonts / 'groups.json').read_text()
    assert (destination / 'LICENSE').read_text() == 'license'
    assert all((fonts / group).exists() for group in groups)


def test_unknown_font_layout_is_copied_intact(tmp_path):
    source = tmp_path / 'source'
    (source / 'fonts/unknown').mkdir(parents=True)
    (source / 'fonts/unknown/font.ttf').write_bytes(b'keep')
    copy_browser_runtime(source, tmp_path / 'destination', 'linux')
    assert (tmp_path / 'destination/fonts/unknown/font.ttf').read_bytes() == b'keep'


def test_legacy_font_deduplication_preserves_distinct_faces_and_notices(tmp_path):
    from scripts.build_native import deduplicate_fonts
    (tmp_path/'a.ttf').write_bytes(b'same face')
    (tmp_path/'b.ttf').write_bytes(b'same face')
    (tmp_path/'c.ttf').write_bytes(b'different face')
    (tmp_path/'NOTICE').write_bytes(b'same face')
    assert deduplicate_fonts(tmp_path)==len(b'same face')
    assert (tmp_path/'a.ttf').exists() and not (tmp_path/'b.ttf').exists()
    assert (tmp_path/'c.ttf').exists() and (tmp_path/'NOTICE').exists()


def test_grouped_font_membership_is_not_changed(tmp_path):
    from scripts.build_native import deduplicate_fonts
    (tmp_path/'groups.json').write_text('{}')
    (tmp_path/'a.ttf').write_bytes(b'same face');(tmp_path/'b.ttf').write_bytes(b'same face')
    assert deduplicate_fonts(tmp_path)==0
    assert (tmp_path/'b.ttf').exists()


def test_windows_font_selection_keeps_collections_unknowns_and_notices(tmp_path,monkeypatch):
    from scripts import build_native
    for name in ('windows.ttf','other.ttf','collection.ttc','unknown.ttf','NOTICE'):
        (tmp_path/name).write_bytes(b'font')
    def names(path):
        if path.name=='unknown.ttf':raise ValueError('unknown font')
        return {'arial','other'} if path.name=='collection.ttc' else {'arial'} if path.name=='windows.ttf' else {'unrelated'}
    monkeypatch.setattr(build_native,'font_families',names)
    assert build_native.trim_legacy_windows_fonts(tmp_path,['Arial'])==4
    assert {p.name for p in tmp_path.iterdir()}=={'windows.ttf','collection.ttc','unknown.ttf','NOTICE'}
