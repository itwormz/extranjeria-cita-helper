import asyncio
from types import SimpleNamespace
import pytest
from playwright.async_api import TimeoutError
from app.page_inspection import visible_documents


def test_missing_body_is_loading_not_fatal():
    class Frame:
        def locator(self, selector):
            return self
        async def inner_text(self, **kwargs):
            raise TimeoutError("body not ready")
    frame = Frame()
    page = SimpleNamespace(frames=[frame], main_frame=frame)
    assert asyncio.run(visible_documents(page)) == [(frame, "")]


def test_closed_page_errors_are_not_hidden():
    class Frame:
        def locator(self, selector):
            return self
        async def inner_text(self, **kwargs):
            raise RuntimeError("page closed")
    frame = Frame()
    page = SimpleNamespace(frames=[frame], main_frame=frame)
    with pytest.raises(RuntimeError, match="page closed"):
        asyncio.run(visible_documents(page))
