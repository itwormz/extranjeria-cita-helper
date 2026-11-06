# Выпуск релиза через PR

Один раз добавьте в GitHub Actions секрет репозитория `RELEASE_TAG_TOKEN`.
Подойдёт fine-grained PAT с доступом только к этому репозиторию и правом
Contents: Read and write. Владелец токена должен иметь право отправлять
коммиты в ветки `release/*` и создавать теги `v*`. GitHub запускает проверки
PR и сборку по событиям, созданным этим токеном.

## Подготовить релиз

1. Обновите локальный `main` командой `git switch main && git pull --ff-only`.
   Создайте ветку `release/0.3.2`, подставив нужную версию.
2. Запишите номер версии одной строкой в `.github/release-version`.
   Например, `0.3.2` без префикса `v`.
3. В `CHANGELOG.md` добавьте готовый текст для страницы GitHub Release.
   Пример раздела:

   ```markdown
   ## Unreleased — Search improvements

   - Show the next eligible appointment attempt in the Work tab.
   - Fix an issue that could interrupt a search when restarting the browser.
   ```

4. Отправьте ветку на GitHub и откройте PR в `main`, например
   `Release: 0.3.2`. В GitHub Actions откройте **Prepare or tag release PR**,
   нажмите **Run workflow**, выберите ветку `release/0.3.2` и запустите.
   Локально ту же подготовку выполняет
   `python3 scripts/prepare_release_version.py` с последующим коммитом и push.
5. Проверьте новый коммит в PR. Версия должна совпадать в `app/version.py`,
   `pyproject.toml` и `uv.lock`. Заметки должны перейти под
   `## 0.3.2 — Search improvements`, а `## Unreleased` остаться пустым.
   Дождитесь обязательных проверок, завершите первый этап ревью Greptile,
   затем дождитесь автоматического запроса CodeRabbit. Порядок проверки исправлений
   и перехода между ревьюерами описан в [инструкции ревью](review-process.md).
   После завершения обоих этапов смержите PR.

После мержа workflow проверяет метаданные и ставит тег `v0.3.2` на коммит
в `main`. Сборка по тегу публикует пакеты, контрольные суммы и аттестации.
Пока сборка идёт, версия в `main` уже обновлена. Готовность релиза проверяйте
на странице GitHub Releases. Если сборка упала после создания тега, откройте
исходный запуск **Build desktop packages** и повторите упавшие задачи.

Перед следующей работой от `main` снова выполните
`git switch main && git pull --ff-only`. Для обычного PR оставьте
`.github/release-version` как есть.
