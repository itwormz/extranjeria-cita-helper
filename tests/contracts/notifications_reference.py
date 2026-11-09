"""Record notification routing and Telegram texts from the Python 0.3.6 implementation.

Usage: python notifications_reference.py --python-root PATH_TO_EXTRACTED_0.3.6 > fixtures/notifications-0.3.6.json

Every expectation is produced by the real Python functions (state_event, EventRouter, make_event,
telegram_message); Rust replays the same inputs and compares.
"""
import argparse
import json
import sys

OFFICE = "Выбор офиса"
BOOKING = "Выбор ситы"
TRANSIENT = "Проверяем страницу"


def load(root):
    sys.path.insert(0, root)
    from app import notifications

    return notifications


def office(document="d1", mode="manual", **extra):
    return dict(run_id="run", document=document, mode=mode, screen=OFFICE, stage="office", **extra)


def transient(document="d1", mode="working", **extra):
    return dict(run_id="run", document=document, mode=mode, screen=TRANSIENT, stage="office", **extra)


def state_cases():
    base = dict(run_id="run", document="d1")
    cases = [
        (dict(base, failed=True, mode="manual", screen=OFFICE), False),
        (dict(base, finished=True, mode="manual", screen=OFFICE), False),
        (dict(base, mode="finished"), False),
        (dict(base, paused=True, mode="manual", screen=OFFICE), False),
        (dict(base, connection_lost=True, mode="manual", screen=OFFICE), False),
        (dict(base, booking_handoff=True, mode="manual"), False),
        (office(), False),
        (office(mode="working"), False),
        (transient(), False),
        (transient(), True),
        (transient(mode="waiting"), True),
        (transient(mode="manual"), True),
        (transient(mode="paused"), True),
        (dict(transient(), document=""), True),
        (dict(transient(), document=None), True),
        (dict(transient(), stage="applicant"), True),
        (dict(base, mode="working", screen="Другая"), True),
        (dict(base, mode="manual", screen=BOOKING), False),
        (dict(base, mode="manual", screen="Капча"), False),
        (dict(base, mode="manual", screen="Авторизация сертификатом"), False),
        (dict(base, mode="manual", screen="Cl@ve Móvil"), False),
        (dict(base, mode="manual", screen="Cl@ve Permanente"), False),
        (dict(base, mode="manual", screen="Авторизация"), False),
        (dict(base, mode="manual", screen="Неизвестный экран"), False),
        ({}, False),
    ]
    return [dict(state=state, include_transient=flag) for state, flag in cases]


def router_scenarios():
    return [
        ("office_once_per_document", [office(), office(), office()]),
        ("office_flicker_working_manual_no_repeat", [office(), transient(), office(), transient(), office()]),
        ("office_new_document_alerts_again", [office("d1"), office("d2"), office("d2"), office("d3")]),
        ("paused_then_resumed_same_office_no_repeat", [office(), office(paused=True), office(paused=True), office()]),
        ("connection_lost_then_back_same_office_no_repeat", [office(), office(connection_lost=True), office()]),
        ("transient_before_manual_office", [transient(), office(), office()]),
        ("office_then_other_run_alerts_again", [office(), dict(office(), run_id="other")]),
        ("booking_handoff_then_details", [
            dict(run_id="run", mode="manual", booking_handoff=True),
            dict(run_id="run", mode="manual", booking_handoff=True, booking_details={"office": "X"}),
        ]),
        ("booking_screen_then_office", [dict(run_id="run", mode="manual", screen=BOOKING), office()]),
        ("auth_screens", [
            dict(run_id="run", mode="manual", screen="Авторизация сертификатом"),
            dict(run_id="run", mode="manual", screen="Cl@ve Móvil"),
            dict(run_id="run", mode="working", screen=TRANSIENT),
            dict(run_id="run", mode="manual", screen="Авторизация сертификатом"),
        ]),
        ("failed_alerts_once", [
            dict(run_id="run", failed=True),
            dict(run_id="run", failed=True),
            dict(run_id="run", failed=True, paused=True),
        ]),
    ]


def telegram_cases():
    details = {
        "office": "Oficina  Uno,  Calle 1",
        "offers": [
            {"date": "2027-01-13", "time": "09:14"},
            {"date": "2027-01-12", "time": "10:00"},
            {"date": "31-02-2027", "time": "10:00"},
            {"date": "2027-01-14", "time": "25:00"},
            "not-a-dict",
        ],
    }
    many = [f"Oficina {index}" for index in range(1, 12)]
    cases = []
    for language in ("ru", "en", "es"):
        cases += [
            ("office_names_whitespace_duplicates_and_non_strings", "office", language, None,
             ["A  B", "A B", 5, None, "C", "   ", "  D\t\n E "]),
            ("office_eleven_names_shows_remainder", "office", language, None, many),
            ("office_empty_list_keeps_plain_message", "office", language, None, []),
            ("office_without_list_keeps_plain_message", "office", language, None, None),
            ("office_only_invalid_names_keeps_plain_message", "office", language, None, [5, None, "  "]),
            ("booking_with_offers", "booking", language, details, None),
            ("booking_without_details", "booking", language, None, None),
            ("booking_details_without_valid_offers", "booking", language, {"office": None, "offers": [{"date": "x"}]}, None),
            ("auth_plain", "auth", language, None, None),
            ("error_plain", "error", language, None, None),
        ]
    cases.append(("office_name_over_240_chars_is_cut", "office", "en", None, ["Z" * 300, "Y"]))
    cases.append(("office_names_do_not_use_booking_details", "office", "en", details, ["Only"]))
    return cases


def run(root):
    notifications = load(root)
    state_events = []
    for case in state_cases():
        state_events.append(dict(case, expect=notifications.state_event(case["state"], include_transient=case["include_transient"])))
    routers = []
    for name, states in router_scenarios():
        router = notifications.EventRouter()
        routers.append(dict(name=name, steps=[dict(state=state, expect=router.observe(state)) for state in states]))
    telegram = []
    for name, kind, language, details, offices in telegram_cases():
        event = notifications.make_event(kind, "Demo", "Madrid", language)
        text = notifications.telegram_message(event, language, details, offices=offices)
        telegram.append(dict(name=name, event=kind, language=language, details=details, offices=offices,
                             plain=event["message"], expect=text))
    return dict(schema=1, source="v0.3.6", state_events=state_events, routers=routers, telegram=telegram,
                pairing_lifetime_seconds=notifications.PAIRING_LIFETIME_SECONDS)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--python-root", required=True)
    arguments = parser.parse_args()
    json.dump(run(arguments.python_root), sys.stdout, ensure_ascii=False, indent=1, sort_keys=True)
    sys.stdout.write("\n")
