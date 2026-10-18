"""Visible frame contents: an embedded error overrides its surrounding form."""
from playwright.async_api import TimeoutError as PlaywrightTimeoutError

from .site import _dead_page_kind, _is_icp_page


def is_icp_frame(frame):
    """srcdoc/about:blank inherit the nearest real parent's origin."""
    while frame is not None:
        if frame.url not in {"about:blank", "about:srcdoc"}:
            return _is_icp_page(frame.url)
        frame = frame.parent_frame
    return False


async def visible_documents(page):
    frames = getattr(page, "frames", None)
    if frames is None:
        return [(page, await page.locator("body").inner_text())]
    documents = []
    for frame in frames:
        if frame != page.main_frame:
            owner = await frame.frame_element()
            if not await owner.is_visible():
                continue
        try:
            text = await frame.locator("body").inner_text(timeout=2_000)
        except PlaywrightTimeoutError:
            # A committing document can temporarily have no body. Treat the
            # whole page as loading, so surrounding forms are not acted upon.
            return [(page.main_frame, "")]
        documents.append((frame, text))
    return documents


async def dead_document(page):
    found = None
    for frame, text in await visible_documents(page):
        kind = _dead_page_kind(text)
        if kind == "session_expired":
            return frame, kind
        if kind is not None:
            found = frame, kind
    return found
