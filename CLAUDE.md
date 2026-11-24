# Project Guidelines

## Issue conventions

- Provide acceptance criteria / deliverables.
- Before the issue is ready to code, describe a solution.
- Do not reference (in a link) the bingle_admin repository, the bingle_notify repository or the user richdrich, you can reference by name here.
- When creating a sub-issue (child of an epic or parent issue), set its milestone to match the parent issue's milestone.

## Test structure (rust)

- Put Rust unit and integration tests in the `tests` directory, in a subfolder matching the `src` subfolder (e.g. tests for `src/api` go in `tests/api`).
- Do not put tests inline with the code — keep them in the test tree (Java-style).
- Mark all tests with `#[test]` and use a separate `#[cfg(...)]` attribute where needed so that IntelliJ can discover them.
- Ensure all tests are referenced in `Cargo.toml`.

## Code conventions (rust)

- Do not use default values on traits outside test-only code. Defaults on traits may be used for testing, but must panic if called in non-test code.
- All state should be in a struct, not global or thread-local, except in very special cases — ask before using global or thread-local storage.
- Always validate that a call returning `Option` succeeds before using the value.

## Code conventions (generic)

- Do not prefix an in-use parameter name with `_` — that prefix signals an intentionally unused parameter.
- Do not use Title Case in comments.
- Expand acronyms on first use in documents.
- Shell commands in docs should be a single line to ease cut/paste/edit.
- Any shell command you reference — in docs, chat, PRs, or issues — must be a single line with no backslash continuations, so it can be copied and pasted and run as-is.

## Worktrees

- The repository root checkout holds the canonical branch for the repo (`deployed`, `staging`, or `master`) and must not be altered directly.
- Do all development in a worktree — the worktree is the prime working checkout.
- This repo's prime working checkout is `.claude/worktrees/staging`.

## Files

- Create temp and output files (logs, etc.) in `tmp/` so they are gitignored.

## Before creating a PR

1. Run the `unit` test target and verify all tests pass with no warnings.
2. Ensure `tests`, `bingle_jsi`, `bingle_local`, `bingle_cli`, and `bingle_webserver` trees all compile.
3. Ensure `scripts/run_quality_checks.sh --strict` passes.

## Post-release checks

- When a PR leaves a check that can only run after a release (on a device, in the app, or on MainNet), add an entry to `POST_RELEASE_CHECKS.md` in the same PR, and write "Part of #N" so the issue stays open.
- After the check passes, close the issue and remove its entry.

## Git notes
- Make all changes through the branch -> PR -> review -> merge workflow. You can land directly on a PR when you don't need input before it.
- *While still modifying a PR*, and if it is incomplete (doesn't yet meet requirements) keep it in draft.
- When the PR is complete and meets all requirements, mark it ready for review/merge.
- Any PR that is not a draft must target the repository's default branch. Keep stacked work in draft with its feature-branch base until that base merges, then retarget the PR to the default branch and mark it ready. A ready PR based on another feature branch merges into that branch — not the default branch — so the work shows as merged on GitHub yet silently never reaches it. Continuous integration can enforce this with a default-branch check.
- **Integration branches (`integration-*`, #491)** test stories together before they release, e.g. `integration-ipv6` for bingle_admin #36. They are a test lane, not a release path: nothing merges from an integration branch into `staging`.
  - A story branch starts from `staging`. Its PR into the integration branch may be marked ready (`pr-base-guard` allows `integration-*`) and is merged there, so the localnet integration suite runs on the combination.
  - The same story releases through its own PR into `staging`: kept in draft, rebased on `staging`, and marked ready when it is its turn to ship. Stories that depend on each other release in order.
  - A fix found on the integration branch goes on the story branch and is merged into the integration branch again. Never commit directly on the integration branch.
  - Rebuild the integration branch from `staging` (re-merging the story branches not yet released) as stories land, instead of merging `staging` into it, so it doesn't collect rebased duplicates of released commits.
  - Delete the integration branch when its last story has released.
- Try to keep the IDE on the working branch, unless for instance you are using an agent in another worktree. Return to staging when the PR is merged and you are idle.
