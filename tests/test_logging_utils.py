from app.logging_utils import register_sensitive, sanitize


def test_sanitizer_masks_token_nie_registered_name_and_url_query():
    token = "123456:abcdefghijklmnopqrstuvwxyzABCDE"
    register_sensitive([token, "SECRET PERSON"])
    text = sanitize(
        f"token={token} nie=X0000000T name=SECRET PERSON "
        "url=https://example.com/path?csrf=secret#fragment"
    )
    assert token not in text
    assert "X0000000T" not in text
    assert "SECRET PERSON" not in text
    assert "csrf" not in text
    assert text.endswith("https://example.com/path")
