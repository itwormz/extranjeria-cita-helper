from html.parser import HTMLParser
from pathlib import Path

from app.classifier import Result, classify_terminal


class VisibleTextParser(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.skip = 0
        self.parts: list[str] = []

    def handle_starttag(self, tag, attrs):
        if tag in {"script", "style"}:
            self.skip += 1

    def handle_endtag(self, tag):
        if tag in {"script", "style"} and self.skip:
            self.skip -= 1

    def handle_data(self, data):
        if not self.skip:
            self.parts.append(data)


def test_public_terminal_negative_fixture_is_no_slots():
    path = Path(__file__).parent / "fixtures" / "no_slots.html"
    parser = VisibleTextParser()
    parser.feed(path.read_text(encoding="utf-8", errors="replace"))
    visible = " ".join(parser.parts)
    result = classify_terminal(path.read_text(errors="replace"), visible_text=visible)
    assert result.result is Result.NO_SLOTS
