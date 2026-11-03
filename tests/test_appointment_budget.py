from datetime import datetime

from app.appointment_budget import AppointmentBudget
from app.appointment_budget_store import BudgetStore
from app.appointment_schedule import MADRID
from app.config import AppointmentProtection


def at(day, hour, minute=0):
    return datetime(2026, 11, day, hour, minute, tzinfo=MADRID)


def test_first_attempt_early_window_and_replan():
    policy = AppointmentProtection()
    budget = AppointmentBudget()
    assert budget.next_submission(at(3, 14), policy) == at(3, 14)
    budget.submitted(at(3, 14), policy)
    assert budget.next_submission(at(3, 14, 10), policy) == at(3, 14, 30)
    budget.submitted(at(3, 14, 40), policy)
    assert budget.windows[0][0] == at(3, 15, 16).replace(second=40)


def test_cod_oper_persists_across_midnight(tmp_path):
    from types import SimpleNamespace

    policy = AppointmentProtection()
    store = BudgetStore(tmp_path, SimpleNamespace(
        id='synthetic', document_type='NIE', document_number='X0000000T',
    ))
    budget = store.load()
    for hour in range(19, 24):
        budget.submitted(at(3, hour, 50), policy)
    budget.cod_oper(at(3, 23, 51), policy)
    store.save(budget)

    restored = store.load()
    assert restored.next_submission(at(4, 0), policy) == at(4, 0, 21)
    assert restored.target(policy) == 4
    assert restored.sent == 0
    assert 'X0000000T' not in store.path.read_text()


def test_zero_target_still_allows_clean_round_after_pause():
    policy = AppointmentProtection()
    budget = AppointmentBudget()
    budget.submitted(at(3, 23, 50), policy)
    budget.cod_oper(at(3, 23, 51), policy)
    assert budget.next_submission(at(4, 0), policy) == at(4, 0, 21)
    assert budget.target(policy) == 0


def test_hot_retry_does_not_grant_another_hot_retry():
    from types import SimpleNamespace

    from app.assist_flow import AssistRunner

    runner = AssistRunner.__new__(AssistRunner)
    runner.budget = AppointmentBudget()
    runner.protection = AppointmentProtection()
    runner.control = SimpleNamespace(save_appointment_budget=lambda budget: None)
    runner.banner_visible = False
    runner.budget_office_recorded = False
    result = SimpleNamespace(kind='post_contact_no_slots', operation_code=False)

    for used_hot_retry, expected in ((False, True), (True, False)):
        runner.used_hot_retry = used_hot_retry
        runner.awaiting_budget_result = True
        runner.round_saw_office = True
        runner.observe_appointment_budget(result)
        assert runner.hot_retry is expected


def test_restriction_history_keeps_office_and_recovery_interval(tmp_path):
    from types import SimpleNamespace

    policy = AppointmentProtection()
    store = BudgetStore(tmp_path, SimpleNamespace(
        id='synthetic', document_type='NIE', document_number='X0000000T',
    ))
    budget = store.load()
    budget.submitted(at(3, 19), policy)
    budget.cod_oper(at(3, 19, 1), policy)
    budget.submitted(at(3, 19, 31), policy)
    budget.office_during_restriction(at(3, 19, 32), ['Демо-офис'])
    assert budget.first_cod_oper is not None
    budget.episode_offices[-1]['result'] = 'post_contact_no_slots'
    budget.ordinary_result(at(3, 19, 33))
    store.save(budget)
    restored = store.load()
    episode = restored.episode_history[-1]
    assert episode['last'] == at(3, 19, 1).isoformat()
    assert episode['recovered'] == at(3, 19, 33).isoformat()
    assert episode['offices'][0]['names'] == ['Демо-офис']
    assert restored.first_cod_oper is None


def test_document_budget_shared_by_profiles_and_normalized(tmp_path):
    from types import SimpleNamespace

    first = BudgetStore(tmp_path, SimpleNamespace(
        id='first', document_type='NIE', document_number='X0000000T',
    ))
    copied = BudgetStore(tmp_path, SimpleNamespace(
        id='second', document_type='nie', document_number=' x0000000t ',
    ))
    assert first.key == copied.key
    budget = first.load()
    budget.submitted(at(3, 14), AppointmentProtection())
    first.save(budget)
    assert copied.load().sent == 1


def test_history_can_be_read_by_profile_after_worker_exit(tmp_path):
    from types import SimpleNamespace

    profile = SimpleNamespace(id='synthetic', document_type='NIE', document_number='X0000000T')
    store = BudgetStore(tmp_path, profile)
    policy = AppointmentProtection()
    budget = store.load()
    budget.submitted(at(3, 19), policy)
    budget.cod_oper(at(3, 19, 1), policy)
    budget.ordinary_result(at(3, 19, 32))
    store.save(budget)
    store.remember_profile(profile.id)
    restored = BudgetStore.load_profile(tmp_path, profile.id)
    assert restored.episode_history[0]['recovered'] == at(3, 19, 32).isoformat()
    assert BudgetStore.load_profile(tmp_path, 'other') is None


def test_desktop_no_slots_restart_uses_get_without_resubmitting():
    import asyncio
    from types import SimpleNamespace
    from unittest.mock import AsyncMock, Mock

    from app.assist_flow import AssistRunner
    from app.config import DEFAULT_START_URL

    runner = AssistRunner.__new__(AssistRunner)
    runner.task = SimpleNamespace(no_slots_mode='reload')
    runner.budget = AppointmentBudget()
    runner.page = SimpleNamespace(goto=AsyncMock(), reload=AsyncMock())
    runner.policy = SimpleNamespace(next_delay=lambda **_: 0)
    runner.control = SimpleNamespace(automation_action=Mock())
    runner.attempted = set()
    runner.empty_streak = 0
    runner.countdown = AsyncMock(return_value=True)
    runner.guard = AsyncMock(return_value=True)
    runner.save = AsyncMock()

    asyncio.run(runner.act(SimpleNamespace(kind='request_no_slots', document='response')))
    runner.page.goto.assert_awaited_once_with(DEFAULT_START_URL, wait_until='domcontentloaded')
    runner.page.reload.assert_not_called()
    assert runner.budget.sent == 0
