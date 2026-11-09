"""Record appointment-budget observation scenarios from the Python 0.3.6 AssistRunner.

Usage: python budget_observer_reference.py --python-root PATH_TO_EXTRACTED_0.3.6 > fixtures/budget-observer-0.3.6.json

Steps run the real `AssistRunner._requested` (a POST to acCitar) and `observe_appointment_budget`; the
province reset and the office record are the two short fragments of `AssistRunner.act` that cannot be
called in isolation and are reproduced here line for line (marked below).
"""
import argparse
import json
import sys
from datetime import datetime, timezone
from types import SimpleNamespace

CURRENT = [None]


def utc_text(value):
    value = value.astimezone(timezone.utc)
    text = value.strftime("%Y-%m-%dT%H:%M:%S")
    if value.microsecond:
        text += f".{value.microsecond:06d}"
    return text + "Z"


def normal(value):
    if isinstance(value, datetime):
        return utc_text(value)
    if isinstance(value, str) and len(value) > 10 and value[4] == "-" and value[10] == "T":
        return utc_text(datetime.fromisoformat(value))
    if isinstance(value, dict):
        return {key: normal(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [normal(item) for item in value]
    return value


def scenarios():
    def m(day, hour, minute=0):
        return (day, hour, minute)

    def step(op, at, **args):
        return dict(op=op, at=at, **args)

    def observe(at, kind, code=False):
        return step("observe", at, kind=kind, operation_code=code)

    out = []

    def add(name, steps, policy=None):
        out.append(dict(name=name, policy=policy or {}, steps=steps))

    add("banner_counted_once_per_visible_period", [
        observe(m(3, 9), "applicant_no_slots"),
        observe(m(3, 9, 1), "applicant_no_slots"),
        observe(m(3, 9, 2), "loading"),
        observe(m(3, 9, 3), "applicant_no_slots"),
        observe(m(3, 9, 4), "request_menu"),
        observe(m(3, 9, 5), "applicant_no_slots"),
        step("province", m(3, 9, 6)),
    ])
    add("office_then_no_slots_grants_one_hot_retry", [
        step("request_sent", m(3, 9)),
        observe(m(3, 9, 1), "office"),
        observe(m(3, 9, 2), "request_no_slots"),
        step("request_sent", m(3, 9, 20)),
        observe(m(3, 9, 21), "office"),
        observe(m(3, 9, 22), "post_contact_no_slots"),
    ])
    add("no_office_seen_no_hot_retry", [
        step("request_sent", m(3, 9)),
        observe(m(3, 9, 1), "request_no_slots_restart"),
    ])
    add("retry_after_office_disabled_no_hot_retry", [
        step("request_sent", m(3, 9)),
        observe(m(3, 9, 1), "office"),
        observe(m(3, 9, 2), "request_no_slots"),
    ], policy=dict(retry_after_office=False))
    add("booking_result_never_grants_hot_retry_or_counts_no_slots", [
        step("request_sent", m(3, 9)),
        observe(m(3, 9, 1), "office"),
        observe(m(3, 9, 2), "booking"),
    ])
    add("result_without_a_sent_request_is_ignored", [
        observe(m(3, 9), "request_no_slots"),
        observe(m(3, 9, 1), "post_contact_no_slots", True),
    ])
    add("cod_oper_result_starts_restriction_and_keeps_no_hot_retry", [
        step("request_sent", m(3, 9)),
        observe(m(3, 9, 1), "office"),
        observe(m(3, 9, 2), "request_no_slots", True),
        observe(m(3, 9, 3), "request_no_slots"),
    ])
    add("office_during_restriction_recorded_once_with_its_result", [
        step("request_sent", m(3, 9)),
        observe(m(3, 9, 1), "request_no_slots", True),
        step("request_sent", m(3, 9, 31)),
        step("offices_seen", m(3, 9, 32), names=["Oficina A", "Oficina B"]),
        step("offices_seen", m(3, 9, 33), names=["Oficina A", "Oficina B"]),
        observe(m(3, 9, 34), "post_contact_no_slots", True),
        step("request_sent", m(3, 10, 5)),
        step("offices_seen", m(3, 10, 6), names=["Oficina C"]),
        observe(m(3, 10, 7), "request_no_slots"),
    ])
    add("province_resets_the_round_banner_flag", [
        observe(m(3, 9), "applicant_no_slots"),
        step("province", m(3, 9, 1)),
        observe(m(3, 9, 2), "province"),
    ])
    add("request_sent_clears_pending_hot_retry_and_remembers_it", [
        step("request_sent", m(3, 9)),
        observe(m(3, 9, 1), "office"),
        observe(m(3, 9, 2), "request_no_slots"),
        step("request_sent", m(3, 9, 40)),
        observe(m(3, 9, 41), "request_no_slots"),
        step("request_sent", m(3, 10, 30)),
        observe(m(3, 10, 31), "office"),
        observe(m(3, 10, 32), "request_no_slots"),
    ])
    return out


def run(root):
    sys.path.insert(0, root)
    from app import assist_flow
    from app.appointment_budget import AppointmentBudget
    from app.appointment_schedule import MADRID
    from app.config import AppointmentProtection

    class FakeDateTime(datetime):
        @classmethod
        def now(cls, tz=None):
            return CURRENT[0].astimezone(tz) if tz else CURRENT[0]

    assist_flow.datetime = FakeDateTime

    def at(spec):
        day, hour, minute = spec
        return datetime(2026, 11, day, hour, minute, tzinfo=MADRID)

    result = []
    for scenario in scenarios():
        policy = AppointmentProtection(**scenario["policy"])
        runner = assist_flow.AssistRunner.__new__(assist_flow.AssistRunner)
        runner.budget = AppointmentBudget()
        runner.protection = policy
        runner.control = SimpleNamespace(
            save_appointment_budget=lambda budget: None, hot_retry=False, publish=lambda **kwargs: None,
            availability_checked=lambda: None, request_sent=lambda: None,
            snapshot=lambda: {"checks": 0, "profile_checks": 0})
        runner.banner_visible = runner.round_saw_banner = runner.awaiting_budget_result = False
        runner.round_saw_office = runner.used_hot_retry = runner.budget_office_recorded = runner.hot_retry = False
        runner.page = SimpleNamespace(main_frame=object())
        runner.last_observed = None
        runner.http_statuses = {}
        runner.last_method = None
        runner.pending_action = None
        runner.pending_counted = False
        runner.record = lambda *args, **kwargs: None
        steps = []
        for raw in scenario["steps"]:
            CURRENT[0] = at(raw["at"])
            op = raw["op"]
            if op == "request_sent":
                request = SimpleNamespace(
                    is_navigation_request=lambda: True, frame=runner.page.main_frame, method="POST",
                    url="https://icp.administracionelectronica.gob.es/icpplus/acCitar", post_data=None)
                runner._requested(request)
            elif op == "observe":
                runner.observe_appointment_budget(SimpleNamespace(kind=raw["kind"], operation_code=raw["operation_code"]))
            elif op == "province":
                runner.round_saw_banner = False  # AssistRunner.act, kind == "province"
            elif op == "offices_seen":
                # AssistRunner.act, office list branch (~line 1205), reproduced line for line.
                offices = [{"text": name} for name in raw["names"]]
                if (runner.budget is not None and runner.budget.first_cod_oper is not None
                        and runner.awaiting_budget_result and not runner.budget_office_recorded):
                    runner.budget.office_during_restriction(
                        assist_flow.datetime.now(MADRID), [office["text"] for office in offices])
                    runner.budget_office_recorded = True
            else:
                raise SystemExit(f"unknown op {op}")
            summary = normal(runner.budget.summary(CURRENT[0], policy))
            flags = dict(
                hot_retry=bool(runner.hot_retry), awaiting_result=bool(runner.awaiting_budget_result),
                round_saw_banner=bool(runner.round_saw_banner), round_saw_office=bool(runner.round_saw_office),
                banner_visible=bool(runner.banner_visible), used_hot_retry=bool(runner.used_hot_retry))
            steps.append(dict(raw, at=utc_text(CURRENT[0]), summary=summary, flags=flags))
        result.append(dict(scenario, steps=steps))
    return dict(schema=1, source="v0.3.6", scenarios=result)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--python-root", required=True)
    arguments = parser.parse_args()
    json.dump(run(arguments.python_root), sys.stdout, ensure_ascii=False, indent=1, sort_keys=True)
    sys.stdout.write("\n")
