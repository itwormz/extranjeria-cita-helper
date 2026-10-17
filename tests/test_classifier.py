from app.classifier import Result, classify_terminal


def test_known_no_slots_is_successful_negative_result():
    result = classify_terminal("<p>En este momento no hay citas disponibles.</p>")
    assert result.result is Result.NO_SLOTS
    assert result.error_code is None


def test_available_requires_actionable_selector():
    result = classify_terminal("<h1>Paso 1 de 5</h1>", {"#idSede"})
    assert result.result is Result.AVAILABLE


def test_unknown_terminal_page_is_error():
    result = classify_terminal("<h1>Contenido nuevo</h1>")
    assert result.result is Result.ERROR
    assert result.error_code == "UNKNOWN_RESULT"


def test_captcha_has_priority_over_unknown():
    result = classify_terminal("Complete el CAPTCHA")
    assert result.result is Result.ERROR
    assert result.error_code == "CAPTCHA_DETECTED"


def test_hidden_captcha_markup_does_not_override_visible_no_slots():
    result = classify_terminal(
        '<input type="hidden" id="alert_captcha" value="captcha incorrecto">',
        visible_text="En este momento no hay citas disponibles.",
    )
    assert result.result is Result.NO_SLOTS
