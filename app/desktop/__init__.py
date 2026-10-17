"""Optional desktop interface. Qt is imported only when it is launched."""


def run_desktop():
    from .window import run_desktop as run
    return run()
