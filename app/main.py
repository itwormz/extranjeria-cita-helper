"""Desktop application and its private worker entrypoint."""
import argparse
import asyncio
import logging
import os
from pathlib import Path
import sys
from .desktop.paths import runtime_root, resource_root
from .logging_utils import configure_logging
ROOT = runtime_root()

def _paths():
    return (Path(os.environ.get('TASKS_FILE', ROOT/'tasks.yml')),
            Path(os.environ.get('CATALOG_FILE', resource_root()/'catalogs/snapshot-2026-08-12.json')),
            ROOT/'data/state.db')

def build_parser():
    parser = argparse.ArgumentParser(prog='extranjeria-cita-helper')
    sub = parser.add_subparsers(dest='command')
    sub.add_parser('desktop')
    worker = sub.add_parser('worker')
    worker.add_argument('--task', required=True)
    return parser

def main(argv=None):
    args = build_parser().parse_args(argv)
    configure_logging(os.environ.get('LOG_LEVEL','INFO'))
    if args.command == 'worker':
        for handler in logging.getLogger().handlers:
            if isinstance(handler, logging.StreamHandler): handler.setStream(sys.stderr)
        from .desktop.worker import run_worker
        return asyncio.run(run_worker(args.task))
    from .desktop import run_desktop
    return run_desktop()

if __name__ == '__main__':
    raise SystemExit(main())
