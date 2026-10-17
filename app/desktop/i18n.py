"""Desktop translations. Source diagnostics stay unchanged on disk."""
import json
from pathlib import Path
import re
from functools import lru_cache

LANGUAGES = {'ru': 'Русский', 'en': 'English', 'es': 'Español'}
CATALOG = json.loads((Path(__file__).parent / 'assets' / 'translations.json').read_text(encoding='utf-8'))


@lru_cache(maxsize=2)
def _pattern(language):
    return re.compile('|'.join(re.escape(k) for k in sorted(CATALOG[language], key=len, reverse=True)))


def translate(text, language):
    if language == 'ru' or not text:
        return text
    table = CATALOG.get(language, {})
    if text in table:
        return table[text]
    # Translate dynamic diagnostics around IDs, paths, timings and site text.
    if not table:
        return text
    pattern = _pattern(language)
    return pattern.sub(lambda match: table[match.group()], text)
