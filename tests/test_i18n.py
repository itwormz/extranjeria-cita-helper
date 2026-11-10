import ast
import re
from pathlib import Path

import pytest

from app.desktop.i18n import translate

CYRILLIC = re.compile('[А-Яа-яЁё]')
# A single word that mixes scripts, e.g. "Untilбавить" or "Fromайт".
MIXED_WORD = re.compile(r'(?=\w*[А-Яа-яЁё])(?=\w*[A-Za-z])\w+')
# Russian text here is intentional: per-language tables, matching patterns or fixtures.
NOT_TRANSLATED = {
    'app/desktop/achievement_exports.py',  # own per-language label tables
    'app/desktop/easter_egg.py',  # own per-language proverbs
    'app/assist_journal.py',  # keyword patterns matched against Russian messages
    'app/desktop/smoke.py',  # offline smoke-test page fixture
    'app/desktop/i18n.py',  # native language names
}
# Literals compared against worker state, never displayed.
NOT_DISPLAYED = {('app/desktop/window.py', 'новым проходом')}


def interface_literals():
    root = Path(__file__).resolve().parents[1]
    found = {}
    for path in sorted((root / 'app').rglob('*.py')):
        relative = path.relative_to(root).as_posix()
        if relative in NOT_TRANSLATED:
            continue
        for node in ast.walk(ast.parse(path.read_text(encoding='utf-8'))):
            if (isinstance(node, ast.Constant) and isinstance(node.value, str) and CYRILLIC.search(node.value)
                    and (relative, node.value) not in NOT_DISPLAYED):
                found.setdefault(node.value, f'{relative}:{node.lineno}')
    return found


@pytest.mark.parametrize('language', ['en', 'es'])
def test_short_keys_do_not_translate_inside_words(language):
    # "До", "С", "По", "с" and "ч" are catalog keys; they must not split other words.
    for word in ('Добавить', 'Сайт', 'Получено', 'чистый', 'сессия'):
        assert not MIXED_WORD.search(translate(word + ' X', language)), word


@pytest.mark.parametrize('language', ['en', 'es'])
def test_standalone_units_still_translate_in_dynamic_text(language):
    assert translate('5 мин 3 с', language) == '5 min 3 s'
    assert translate('2 ч', language) == '2 h'


@pytest.mark.parametrize('language', ['en', 'es'])
def test_interface_strings_never_render_mixed_script_words(language):
    broken = {where: translate(text, language) for text, where in interface_literals().items()
              if MIXED_WORD.search(translate(text, language))}
    assert broken == {}


@pytest.mark.parametrize('language', ['en', 'es'])
def test_interface_strings_are_fully_translated(language):
    missing = {where: text for text, where in interface_literals().items()
               if CYRILLIC.search(translate(text, language))}
    assert missing == {}
