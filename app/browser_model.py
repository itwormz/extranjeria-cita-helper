"""Keep fpgen's compressed model readable from Windows Unicode paths."""
import sys

from indexed_zstd import IndexedZstdFile


class UnicodeIndexedZstdFile(IndexedZstdFile):
    """Use Python's Unicode-aware open; the native reader borrows its fd."""

    def __init__(self, filename):
        self._source = None
        self._source = open(filename, 'rb')
        try:
            super().__init__(self._source.fileno())
        except BaseException:
            self._source.close()
            raise

    def close(self):
        if self._source is None or self._source.closed:
            return
        try:
            super().close()
        finally:
            self._source.close()


def prepare_browser_model():
    if sys.platform != 'win32':
        return
    # Keep Camoufox's pinned-model validation before any fpgen import.
    from camoufox.fpgen_model import load_fpgen
    load_fpgen()
    from fpgen import unpacker
    # fpgen otherwise passes UTF-8 bytes to indexed_zstd's Windows _open.
    unpacker.IndexedZstdFile = UnicodeIndexedZstdFile
