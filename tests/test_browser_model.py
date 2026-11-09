import os
import sys

import pytest


MODEL = bytes.fromhex('28b52ffd2016b1000066696e6765727072696e742d6d6f64656c2d74657374')


def test_compressed_model_in_unicode_directory_uses_owned_descriptor(tmp_path):
    from app.browser_model import UnicodeIndexedZstdFile

    directory = tmp_path / 'Загрузки Español'
    directory.mkdir()
    model = directory / 'values.dat.zst'
    model.write_bytes(MODEL)
    reader = UnicodeIndexedZstdFile(model)
    descriptor = reader.fileno()
    assert isinstance(reader.raw.name, int)
    assert reader.read(11) == b'fingerprint'
    reader.seek(12)
    assert reader.read() == b'model-test'
    os.fstat(descriptor)
    reader.close()
    reader.close()
    with pytest.raises(OSError):
        os.fstat(descriptor)


def test_invalid_compressed_model_closes_source(monkeypatch, tmp_path):
    from app import browser_model

    model = tmp_path / 'values.dat.zst'
    model.write_bytes(b'not a zstd frame')
    source = model.open('rb')
    monkeypatch.setattr(browser_model, 'open', lambda *_: source, raising=False)
    with pytest.raises(ValueError):
        browser_model.UnicodeIndexedZstdFile(model)
    assert source.closed


def test_missing_model_preserves_file_error(tmp_path):
    from app.browser_model import UnicodeIndexedZstdFile

    with pytest.raises(FileNotFoundError):
        UnicodeIndexedZstdFile(tmp_path / 'missing.zst')


def test_windows_fpgen_reads_through_unicode_adapter(monkeypatch, tmp_path):
    from types import ModuleType
    from camoufox import fpgen_model
    from indexed_zstd import IndexedZstdFile
    from app import browser_model

    # Avoid fpgen's import-time model download. Exercise its exact reader boundary.
    fpgen = ModuleType('fpgen')
    unpacker = ModuleType('fpgen.unpacker')
    unpacker.IndexedZstdFile = IndexedZstdFile
    fpgen.unpacker = unpacker
    validated = []
    monkeypatch.setattr(fpgen_model, 'load_fpgen', lambda: validated.append(True))
    monkeypatch.setitem(sys.modules, 'fpgen', fpgen)
    monkeypatch.setattr(browser_model.sys, 'platform', 'win32')
    browser_model.prepare_browser_model()
    assert validated == [True]
    model = tmp_path / 'модель.zst'
    model.write_bytes(MODEL)
    with unpacker.IndexedZstdFile(str(model)) as reader:
        assert isinstance(reader.raw.name, int)
        assert reader.read() == b'fingerprint-model-test'


@pytest.mark.skipif(sys.platform != 'win32', reason='Windows narrow-path regression')
def test_windows_native_path_fails_but_unicode_reader_works(tmp_path):
    from indexed_zstd import IndexedZstdFile
    from app.browser_model import UnicodeIndexedZstdFile

    directory = tmp_path / 'Загрузки'
    directory.mkdir()
    model = directory / 'values.dat.zst'
    model.write_bytes(MODEL)
    try:
        original = IndexedZstdFile(str(model))
    except ValueError as exc:
        assert 'ZSTDSeekContext' in str(exc)
    else:
        original.close()
        pytest.skip('The Windows code page already supports this UTF-8 path')
    with UnicodeIndexedZstdFile(model) as reader:
        assert reader.read() == b'fingerprint-model-test'
