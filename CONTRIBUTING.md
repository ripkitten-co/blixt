# Contributing to Blixt

## Dev Setup

```bash
git clone https://github.com/ripkitten-co/blixt.git
cd blixt
cargo build
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

The workspace has two crates: `blixt` (core framework) and `blixt-cli`. Both are built and tested together.

## Branching

Branch from `develop`. Never push directly to `main` or `develop` — all changes go through pull requests.

Branch naming:

- `feature/short-description`
- `fix/short-description`
- `docs/short-description`
- `refactor/short-description`

## Commits

Use conventional commits:

- `feat:` new feature
- `fix:` bug fix
- `docs:` documentation
- `test:` tests
- `chore:` maintenance, deps, config
- `refactor:` restructuring without behavior change

Keep the subject under 72 characters. Focus on *why*, not *what*.

## Pull Requests

Before opening a PR:

- `cargo fmt` — no formatting issues
- `cargo clippy -- -D warnings` — no warnings
- `cargo test` — all tests pass

In the PR description, explain what changed and why. Keep PRs focused on a single concern — don't mix a feature with an unrelated refactor.

CI runs fmt, clippy, and tests automatically. All checks must pass before merge.

## What to Contribute

Bug fixes, test coverage, and documentation improvements are always welcome.

For larger features or design changes, open an issue first so we can discuss the approach before you spend time on it.

## Code Style

Follow the patterns already in the codebase. Run `cargo fmt` and `cargo clippy -- -D warnings` before committing. If you're unsure about something, look at how similar code is structured in the project.

## Code of Conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md). Be kind, be respectful.
