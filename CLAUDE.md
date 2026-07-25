# Project Guidelines

## Test structure

- Put Rust unit and integration tests in the `tests` directory, in a subfolder matching the `src` subfolder (e.g. tests for `src/api` go in `tests/api`).
- Do not put tests inline with the code — keep them in the test tree (Java-style).
- Mark all tests with `#[test]` and use a separate `#[cfg(...)]` attribute where needed so that IntelliJ can discover them.
- Ensure all tests are referenced in `Cargo.toml`.

## Code conventions

- Do not use default values on traits outside test-only code.
- All state should be in a struct, not global or thread-local, except in very special cases — ask before using global or thread-local storage.
- Always validate that a call returning `Option` succeeds before using the value.
- Do not prefix an in-use parameter name with `_` — that prefix signals an intentionally unused parameter.
- Do not use Title Case in comments.
- where there are worktrees in a checked out repo, the root directory of the repo should have `deployed` checked out and should not be altered

## Files

- Create temp and output files (logs, etc.) in `tmp/` so they are gitignored.

## Before creating a PR

1. Run the `unit` test target and verify all tests pass with no warnings.
2. Ensure `tests`, `bingle_jsi`, `bingle_local`, and `bingle_webserver` trees all compile.
3. Ensure `scripts/run_quality_checks.sh --strict` passes

## Git notes

- if you are still modifying a PR, put it in draft until it is ready for review/merge
