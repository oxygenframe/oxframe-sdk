# Перед пушем релиза

SDK, CLI и сайт выходят одной версией.

Переключатель документации уже обновляется сам: сборка сайта спрашивает
crates.io и переписывает `apps/site/src/lib/versions.json`
(`oxframe-web/apps/site/scripts/crate-versions.ts`). Если crates.io недоступен,
остаётся прошлый список.

Пин плейграунда и compile-образа так не двигается. Образ скачивает с crates.io
ровно ту версию, которая записана в `apps/site/src/lib/sdk.ts` и в
`ARG OXFRAME_SDK_VERSION` Dockerfile. Сайт и образ переключаются вместе и
только после того, как эта версия уже опубликована. Раньше сборка образа не
находит крейт, а живой плейграунд получает 422 от сервиса на старой версии.

## Чеклист

- [ ] В `CHANGELOG.md` есть секция этой версии, `version` в `Cargo.toml` ей равен.
- [ ] `cargo test --workspace --locked` проходит.
- [ ] `oxframe-cli` на той же версии: `Cargo.toml`, `SDK_VERSION` в `src/init.rs`, `CHANGELOG.md`.
- [ ] Тег `vX.Y.Z` запушен в `oxframe-sdk`. Push тега публикует крейты (`.github/workflows/release.yml`).
- [ ] Тот же тег запушен в `oxframe-cli` после тега SDK: релиз CLI забирает SDK по этому тегу.
- [ ] Версия видна на crates.io (`https://crates.io/api/v1/crates/oxframe`).
- [ ] `oxframe-web` переведён на эту версию:
  - `apps/site/src/lib/sdk.ts` — `SDK_VERSION`
  - `services/compile-cf/Dockerfile` — `ARG OXFRAME_SDK_VERSION` (должен совпасть с `sdk.ts`; это проверяет `scripts/gen-examples.ts`)
  - `services/compile/bench/load.ts` — `sdk_version`
  - `bun services/compile-cf/scripts/gen-examples.ts` — переписывает `src/examples.gen.ts`
- [ ] Push `oxframe-web`. `deploy-compile.yml` пересобирает образ, потому что меняется `sdk.ts`.

Версию, которой ещё нет на crates.io, в `sdk.ts` не ставить.
