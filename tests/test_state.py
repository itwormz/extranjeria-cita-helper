from pathlib import Path
from app.state import StateStore

def test_task_lock_is_exclusive(tmp_path: Path):
    store = StateStore(tmp_path / "state.db")
    second_store = StateStore(tmp_path / "state.db")

    with store.task_lock("task_1") as first_acquired:
        with second_store.task_lock("task_1") as second_acquired:
            assert first_acquired is True
            assert second_acquired is False
