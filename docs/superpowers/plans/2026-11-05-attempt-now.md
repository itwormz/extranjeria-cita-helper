# Attempt now Implementation Plan

**Goal:** Однократно снять ожидание окна/cod. oper с сохранением остальных таймингов.
**Architecture:** Флаг в AppointmentBudget, команда worker, пункты существующих меню. Пересчёт restriction_until при смене защиты в step.
**Tech Stack:** Python, PySide6, pytest.
**Spec:** ../specs/2026-11-05-attempt-now-design.md

- [x] Добавить падающие тесты в test_appointment_budget.py и test_profile_timings.py: минимальная пауза сохраняется, разрешение расходуется/сбрасывается, обе паузы пересчитываются от прежнего начала.
- [x] Реализовать бюджет, команду appointment_try_now и пересчёт при сохранении настроек.
- [x] Добавить пункты меню работы/статистики, переводы, журнал ручного действия и тест UI.
- [x] Проверить целевые тесты, полный pytest, diff и документацию. Не мержить PR.
