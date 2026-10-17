"""Profile-owned assist timings; ranges are sampled uniformly on each action."""
from dataclasses import dataclass
import math
import random


@dataclass(frozen=True)
class AssistTimings:
    action_delay_seconds: tuple[float, float] = (.3, .8)
    bootstrap_seconds: tuple[float, float] = (5, 8)
    typing_ms: tuple[float, float] = (70, 140)
    contact_typing_ms: tuple[float, float] = (35, 70)
    reload_seconds: tuple[float, float] = (30, 60)
    navigation_seconds: float = 45
    clave_error_seconds: float = 180
    rate_limit_seconds: float = 180
    no_slots_restart_seconds: float = 180
    alert_seconds: float = 3
    recovery_jitter_seconds: tuple[float, float] = (0, 0)
    expired_seconds: tuple[float, ...] = (0, 30, 60, 120, 300)
    server_button_seconds: tuple[float, ...] = (0, 30, 60, 120, 300)
    server_retry_seconds: tuple[float, ...] = (30, 60, 120, 300)
    blocked_seconds: tuple[float, ...] = (300, 360, 420, 540, 600)
    timeout_retry_seconds: tuple[float, ...] = (2, 5)
    post_timeout_seconds: float = 5
    restart_seconds: float = 5

    def recovery_delay(self, values, attempt=0):
        base = values[min(attempt, len(values)-1)] if isinstance(values, tuple) else values
        return base + random.uniform(*self.recovery_jitter_seconds)


# Key, user-facing label, lower and upper bounds. Tuples longer than two
# entries are retry schedules: the last interval is reused thereafter.
TIMING_FIELDS = (
    ('action_delay_seconds', 'Между действиями, с', 0, 10),
    ('bootstrap_seconds', 'Начальная загрузка / F5, с', 0, 120),
    ('typing_ms', 'Между символами NIE и имени, мс', 0, 1000),
    ('contact_typing_ms', 'Между символами контактов, мс', 0, 1000),
    ('reload_seconds', 'Проверка сит: интервал, с', 5, 3600),
    ('navigation_seconds', 'Таймаут загрузки страницы, с', 5, 300),
    ('no_slots_restart_seconds', 'Нет сит с Aceptar: пауза нового прохода, с', 30, 3600),
    ('clave_error_seconds', 'Ошибка Cl@ve: пауза перед новым окном, с', 30, 3600),
    ('rate_limit_seconds', 'HTTP 429: пауза перед пробой, с', 180, 3600),
    ('alert_seconds', 'Повтор звукового сигнала, с', 1, 60),
    ('recovery_jitter_seconds', 'Джиттер восстановления: добавочная пауза, с', 0, 300),
    ('expired_seconds', 'Истёкшая сессия: паузы попыток, с', 0, 3600),
    ('server_button_seconds', 'Кнопка ошибки: паузы попыток, с', 0, 3600),
    ('server_retry_seconds', 'Ошибка сервера без кнопки: паузы, с', 1, 3600),
    ('blocked_seconds', 'Блокировка: паузы новых сессий, с', 300, 7200),
    ('timeout_retry_seconds', 'Клик не отправлен: паузы повторов, с', .1, 60),
    ('post_timeout_seconds', 'Клик отправлен: ожидание результата, с', 1, 60),
    ('restart_seconds', 'Таймаут действия: пауза перезапуска, с', 1, 60),
)
SEQUENCE_KEYS = {'expired_seconds', 'server_button_seconds', 'server_retry_seconds', 'blocked_seconds', 'timeout_retry_seconds'}


def parse_timings(raw):
    if raw is None:
        raw = {}
    if not isinstance(raw, dict):
        raise ValueError('timings must be a mapping')
    defaults = AssistTimings()
    result = {}
    for key, title, lower, upper in TIMING_FIELDS:
        value = raw.get(key, getattr(defaults, key))
        if value is None or value == '':
            value = getattr(defaults, key)
        multiple = isinstance(getattr(defaults, key), tuple)
        if multiple and (not isinstance(value, (list, tuple)) or not value or len(value) > 10):
            raise ValueError(title + ': expected a nonempty list (up to 10 values)')
        values = value if multiple else [value]
        if any(isinstance(v, bool) for v in values):
            raise ValueError(title + ': expected numbers')
        numbers = tuple(float(v) for v in values)
        if not all(math.isfinite(v) and lower <= v <= upper for v in numbers):
            raise ValueError(f'{title}: {lower}–{upper}')
        if multiple and key not in SEQUENCE_KEYS and (len(numbers) != 2 or numbers[0] > numbers[1]):
            raise ValueError(title + ': minimum must not exceed maximum')
        result[key] = numbers if multiple else numbers[0]
    return AssistTimings(**result)
