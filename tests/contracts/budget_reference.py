"""Record appointment-budget scenarios from the Python 0.3.6 implementation.

Usage: python budget_reference.py --python-root PATH_TO_EXTRACTED_0.3.6 > fixtures/budget-0.3.6.json

Every step is executed on the real Python AppointmentBudget; the observed result is stored as
`expect`. Rust replays the same steps and compares. Times are authored in Europe/Madrid (like the
Python tests) and written as UTC.
"""
import argparse
import json
import re
import sys
from datetime import date, datetime, timezone

ISO = re.compile(r"^\d{4}-\d{2}-\d{2}T")


def load(root):
    sys.path.insert(0, root)
    from app.appointment_budget import AppointmentBudget
    from app.appointment_schedule import MADRID
    from app.config import AppointmentProtection

    return AppointmentBudget, AppointmentProtection, MADRID


def utc_text(value):
    value = value.astimezone(timezone.utc)
    text = value.strftime("%Y-%m-%dT%H:%M:%S")
    if value.microsecond:
        text += f".{value.microsecond:06d}"
    return text + "Z"


def normal(value):
    if isinstance(value, datetime):
        return utc_text(value)
    if isinstance(value, str) and ISO.match(value):
        return utc_text(datetime.fromisoformat(value))
    if isinstance(value, dict):
        return {key: normal(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [normal(item) for item in value]
    return value


def scenarios():
    def m(day, hour, minute=0, second=0):
        return (day, hour, minute, second)

    def step(op, at, **args):
        return dict(op=op, at=at, **args)

    s = []

    def add(name, group, steps, policy=None, initial=None):
        s.append(dict(name=name, group=group, policy=policy or {}, initial=initial, steps=steps))

    add("existing_profiles_default_to_no_windows", "distribution", [
        step("submitted", m(3, 9)),
        step("next_submission", m(3, 9, 10), hot=False),
        step("next_submission", m(3, 9, 16), hot=False),
        step("summary", m(3, 9, 10), hot=False),
    ])
    add("opt_in_windows_and_daily_toggle", "distribution", [
        step("submitted", m(3, 9)),
        step("next_submission", m(3, 9, 20), hot=False),
        step("summary", m(3, 9, 20), hot=False),
        step("set_distribution", m(3, 9, 20), disabled=True),
        step("next_submission", m(3, 9, 20), hot=False),
        step("cod_oper", m(3, 9, 20)),
        step("summary", m(3, 9, 21), hot=False),
        step("next_submission", m(3, 9, 21), hot=False),
    ], policy=dict(distribute_attempts=True))
    add("today_enable_is_optional_and_expires_at_midnight", "distribution", [
        step("submitted", m(3, 9)),
        step("set_distribution", m(3, 9, 20), disabled=False),
        step("summary", m(3, 9, 20), hot=False),
        step("next_submission", m(3, 9, 20), hot=False),
        step("refresh", m(4, 0)),
        step("summary", m(4, 0), hot=False),
    ])
    add("try_now_bypasses_window_and_cod_pause", "try_now", [
        step("submitted", m(3, 12)),
        step("cod_oper", m(3, 12, 1)),
        step("try_now", m(3, 12, 2)),
        step("next_submission", m(3, 12, 2), hot=False),
        step("summary", m(3, 12, 2), hot=False),
        step("submitted", m(3, 12, 15)),
        step("next_submission", m(3, 12, 16), hot=False),
    ], policy=dict(minimum_pause_minutes=15))
    add("try_now_one_submission_then_replans_and_expires", "try_now", [
        step("submitted", m(3, 9)),
        step("try_now", m(3, 9, 31)),
        step("next_submission", m(3, 9, 31), hot=False),
        step("summary", m(3, 9, 31), hot=False),
        step("submitted", m(3, 9, 31)),
        step("next_submission", m(3, 9, 32), hot=False),
        step("try_now", m(3, 23)),
        step("refresh", m(4, 0)),
        step("summary", m(4, 0), hot=False),
    ], policy=dict(distribute_attempts=True))
    add("changed_cod_timings_recalculate_from_original_start", "recalc", [
        step("submitted", m(3, 12)),
        step("cod_oper", m(3, 12, 1)),
        step("recalculate_restriction", m(3, 12, 1), policy=dict(first_cod_pause_minutes=10, next_cod_pause_minutes=20)),
        step("restriction_until", m(3, 12, 1)),
        step("submitted", m(3, 13)),
        step("cod_oper", m(3, 13, 1)),
        step("recalculate_restriction", m(3, 13, 1), policy=dict(first_cod_pause_minutes=10, next_cod_pause_minutes=20)),
        step("restriction_until", m(3, 13, 1)),
    ])
    add("first_attempt_early_window_and_replan", "distribution", [
        step("next_submission", m(3, 14), hot=False),
        step("submitted", m(3, 14)),
        step("next_submission", m(3, 14, 10), hot=False),
        step("submitted", m(3, 14, 40)),
        step("summary", m(3, 14, 41), hot=False),
    ], policy=dict(distribute_attempts=True))
    add("cod_oper_persists_across_midnight", "base", [
        step("submitted", m(3, 19, 50)),
        step("submitted", m(3, 20, 50)),
        step("submitted", m(3, 21, 50)),
        step("submitted", m(3, 22, 50)),
        step("submitted", m(3, 23, 50)),
        step("cod_oper", m(3, 23, 51)),
        step("next_submission", m(4, 0), hot=False),
        step("summary", m(4, 0), hot=False),
    ])
    add("zero_target_still_allows_clean_round_after_pause", "base", [
        step("submitted", m(3, 23, 50)),
        step("cod_oper", m(3, 23, 51)),
        step("next_submission", m(4, 0), hot=False),
        step("summary", m(4, 0), hot=False),
    ])
    add("restriction_history_keeps_office_and_recovery_interval", "base", [
        step("submitted", m(3, 19)),
        step("cod_oper", m(3, 19, 1)),
        step("submitted", m(3, 19, 31)),
        step("office_during_restriction", m(3, 19, 32), names=["Демо-офис"]),
        step("set_last_office_result", m(3, 19, 32), result="post_contact_no_slots"),
        step("ordinary_result", m(3, 19, 33)),
        step("summary", m(3, 19, 34), hot=False),
    ])
    add("cod_oper_count_and_next_target", "base", [
        step("submitted", m(3, 10)),
        step("cod_oper", m(3, 10, 1)),
        step("cod_oper", m(3, 10, 2)),
        step("summary", m(3, 10, 3), hot=False),
    ])
    add("clock_goes_backwards_never_returns_before_now", "base", [
        step("submitted", m(3, 12)),
        step("next_submission", m(3, 11, 30), hot=False),
        step("summary", m(3, 11, 30), hot=False),
    ])
    add("adaptive_target_applies_only_to_the_next_day", "floor_and_manual", [
        step("submitted", m(3, 19)),
        step("cod_oper", m(3, 19, 1)),
        step("summary", m(4, 0), hot=False),
        step("summary", m(5, 0), hot=False),
    ], policy=dict(daily_target=8))
    add("adaptive_floor_and_manual_override_preserve_restriction", "floor_and_manual", [
        step("cod_oper", m(3, 12)),
        step("summary", m(3, 12), hot=False),
        step("set_daily_target", m(3, 12), target=20),
        step("summary", m(3, 12), hot=False),
        step("set_daily_target", m(3, 12), target=None),
        step("summary", m(3, 12), hot=False),
        step("set_daily_target", m(3, 12), target=20),
        step("refresh", m(4, 0)),
        step("summary", m(4, 0), hot=False),
    ], policy=dict(daily_target=50), initial=dict(day="2026-11-03", sent=3, target_override=1))
    add("manual_budget_keeps_collecting_tomorrows_adaptive_estimate", "floor_and_manual", [
        step("set_daily_target", m(3, 12), target=40),
        step("cod_oper", m(3, 12)),
        step("summary", m(3, 12), hot=False),
        step("set_daily_target", m(3, 12), target=None),
        step("summary", m(3, 12), hot=False),
        step("refresh", m(4, 0)),
        step("summary", m(4, 0), hot=False),
    ], initial=dict(day="2026-11-03", target_override=10, sent=7))
    add("manual_target_rejects_invalid_values", "floor_and_manual", [
        step("set_daily_target", m(3, 12), target=0),
        step("set_daily_target", m(3, 12), target=-1),
        step("set_daily_target", m(3, 12), target=2147483648),
        step("set_daily_target", m(3, 12), target=True),
        step("summary", m(3, 12), hot=False),
    ])
    add("large_target_keeps_schedule_memory_bounded", "large_target", [
        step("set_daily_target", m(3, 12), target=2147483647),
        step("summary", m(3, 12), hot=False),
    ], policy=dict(minimum_pause_minutes=1, distribute_attempts=True), initial=dict(day="2026-11-03", sent=1))
    return s


def run(root):
    Budget, Protection, MADRID = load(root)

    def at(spec):
        day, hour, minute, second = spec
        return datetime(2026, 11, day, hour, minute, second, tzinfo=MADRID)

    out = []
    for scenario in scenarios():
        initial = scenario["initial"]
        if initial is None:
            budget = Budget()
        else:
            fields = dict(initial)
            fields["day"] = date.fromisoformat(fields["day"])
            budget = Budget(**fields)
        steps = []
        for raw in scenario["steps"]:
            raw = dict(raw)
            op = raw["op"]
            when = at(raw["at"])
            policy = Protection(**{**scenario["policy"], **raw.get("policy", {})})
            record = dict(raw, at=utc_text(when))
            try:
                if op == "submitted":
                    budget.submitted(when, policy)
                elif op == "cod_oper":
                    budget.cod_oper(when, policy)
                elif op == "ordinary_result":
                    budget.ordinary_result(when)
                elif op == "try_now":
                    budget.try_now(when, policy)
                elif op == "refresh":
                    budget.refresh(when, policy)
                elif op == "recalculate_restriction":
                    budget.recalculate_restriction(policy)
                elif op == "office_during_restriction":
                    budget.office_during_restriction(when, raw["names"])
                elif op == "set_last_office_result":
                    budget.episode_offices[-1]["result"] = raw["result"]
                elif op == "set_daily_target":
                    budget.set_daily_target(raw["target"], when, policy)
                elif op == "set_distribution":
                    budget.set_distribution(raw["disabled"], when, policy)
                elif op == "next_submission":
                    record["expect"] = normal(budget.next_submission(when, policy, hot=raw["hot"]))
                elif op == "summary":
                    record["expect"] = normal(budget.summary(when, policy, hot=raw["hot"]))
                elif op == "restriction_until":
                    record["expect"] = normal(budget.restriction_until)
                else:
                    raise SystemExit(f"unknown op {op}")
            except ValueError:
                record["expect_error"] = True
            steps.append(record)
        out.append(dict(scenario, steps=steps))
    return dict(schema=1, source="v0.3.6", scenarios=out)


def emit_file(root):
    """A budget file exactly as Python 0.3.6 BudgetStore writes it, plus the summary Python reads from it."""
    from dataclasses import asdict

    Budget, Protection, MADRID = load(root)
    policy = Protection()

    def at(day, hour, minute=0):
        return datetime(2026, 11, day, hour, minute, tzinfo=MADRID)

    budget = Budget()
    budget.submitted(at(3, 19), policy)
    budget.cod_oper(at(3, 19, 1), policy)
    budget.submitted(at(3, 19, 31), policy)
    budget.office_during_restriction(at(3, 19, 32), ["Демо-офис"])
    budget.episode_offices[-1]["result"] = "post_contact_no_slots"
    budget.ordinary_result(at(3, 19, 33))
    budget.set_daily_target(20, at(3, 19, 34), policy)
    budget.submitted(at(3, 20), policy)
    stored = json.loads(json.dumps(asdict(budget), default=lambda value: value.isoformat()))
    now = at(3, 20, 5)
    summary = normal(budget.summary(now, policy))
    return dict(schema=1, source="v0.3.6", file=stored, policy={}, at=utc_text(now), summary=summary)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--python-root", required=True)
    parser.add_argument("--emit-file", action="store_true", help="emit one stored budget file instead of scenarios")
    arguments = parser.parse_args()
    result = emit_file(arguments.python_root) if arguments.emit_file else run(arguments.python_root)
    json.dump(result, sys.stdout, ensure_ascii=False, indent=1, sort_keys=True)
    sys.stdout.write("\n")
