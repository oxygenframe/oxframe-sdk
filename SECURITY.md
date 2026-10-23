# Security Policy

## Supported versions

oxframe is pre-1.0 and moves fast. Security fixes land in the latest release
only — pin an exact version in `Cargo.toml` and upgrade to pick them up.

## Reporting a vulnerability

Please report security issues privately, not in a public issue. Use GitHub's
private vulnerability reporting: the **Security** tab of this repository →
**Report a vulnerability**. We aim to acknowledge a report within a few days
and will credit you in the fix unless you prefer otherwise.

The playground on [oxframe.dev](https://oxframe.dev/playground) compiles
user-supplied Rust on a server behind a sandbox. If you find a way to escape
that sandbox, read files you should not, or exhaust its limits, please report
it the same way.
