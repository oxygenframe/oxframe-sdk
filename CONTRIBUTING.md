# Contributing to oxframe

Thanks for your interest. oxframe is pre-1.0 and changing quickly — the
[changelog](CHANGELOG.md) shows where it has been, and the pre-1.0 banner on
every page of [oxframe.dev](https://oxframe.dev) is a reminder that the API is
not stable yet.

Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md).
Report a problem to [conduct@oxframe.dev](mailto:conduct@oxframe.dev).

## Before a big change

Open an issue or a GitHub Discussion first, so we can agree on the shape of it.
Small, self-contained fixes — a bug, a doc, a test — can go straight to a pull
request.

## Building and testing

All crates and `oxframe-cli` share one version. From a clone:

```sh
cargo test --release
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

`cargo run --release -p oxframe-examples --bin <name> -- render|preview|info`
runs one of the bundled examples. The design principles the code follows —
a simple outer API, one responsibility per crate, every large feature behind a
cargo feature — are written down in [`AGENTS.md`](AGENTS.md).

## Pull requests

- **Keep the user-facing API simple.** A typical scene should read as one clear
  line; user code should not need `Box::pin`, explicit lifetimes, `Arc`/`RefCell`
  or `.unwrap()` for conversions. Write a new API first as the scene example you
  wish you could write, then make it real.
- **Show it as a scene.** Add or update an example for any new API and run
  `oxframe check` on it; keep the galleries and docs in step.
- **Name breaking changes** in the PR. The changelog records them "before →
  after", and all crates bump together.

Releasing is maintainer-run and documented in [`AGENTS.md`](AGENTS.md) under
"Releasing" (`scripts/release-check.sh` gates it).
