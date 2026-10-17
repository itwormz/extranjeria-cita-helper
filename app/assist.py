"""Shared helpers for the desktop runner."""
import logging
import random
from dataclasses import dataclass
from .logging_utils import log_event
log = logging.getLogger(__name__)
BOOKING_SELECTORS = ("input[name*='fecha']", "select[name*='fecha']", "input[name*='hora']", "select[name*='hora']", ".ui-datepicker", "input[type='radio'][name*='cita']")
CAPTCHA_SELECTORS = ("#captcha", "input[name*='captcha' i]", "img[src*='captcha' i]")

@dataclass
class ReusePolicy:
    reload_seconds: tuple[float, float] = (30.0, 60.0)
    def next_delay(self, *, recent_activity=False, empty_streak=0):
        return random.uniform(*self.reload_seconds)

def reuse_policy_from_settings(settings):
    return ReusePolicy(settings.assist_timings.reload_seconds)

async def _document_key(page) -> str:
    # DOM attribute is shared by Camoufox's isolated and main worlds. Unlike
    # the URL, this token changes on F5 and on same-URL POST navigation.
    return await page.evaluate("""(() => {
      const root = document.documentElement;
      if (!root.hasAttribute('data-cita-document'))
        root.setAttribute('data-cita-document', crypto.randomUUID());
      return root.getAttribute('data-cita-document');
    })()""")


def _install_lifecycle_logging(page, context, task_id, run_id):
    for owner, event, name in (
        (page, "close", "assist_page_closed"),
        (page, "crash", "assist_page_crashed"),
        (context, "close", "assist_context_closed"),
        (getattr(context, "browser", None), "disconnected", "assist_browser_disconnected"),
    ):
        if owner is not None and hasattr(owner, "on"):
            owner.on(event, lambda *_args, name=name: log_event(
                log, logging.INFO, name, task_id=task_id, run_id=run_id))
