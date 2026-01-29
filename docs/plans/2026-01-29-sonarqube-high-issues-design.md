# SonarCloud High-Issue Cleanup Design

## Goal
Clear all SonarCloud HIGH issues (CRITICAL + MAJOR) with minimal behavior change.

## Scope
- Refactor flagged JavaScript test functions to reduce cognitive complexity.
- Refactor inline JS in `webtor-demo/static/index.html` to reduce cognitive complexity.
- Replace POSIX `[` conditionals with `[[` in flagged shell scripts.
- Fix accessibility label issues in `webtor-demo/static/index.html`.

## Non-Goals
- No new modules or build steps.
- No change to runtime behavior or test semantics.
- No reformatting or unrelated cleanup.

## Approach
- Keep changes localized in existing files.
- Split complex functions into small helpers within the same file.
- Keep inline UI script in HTML, but extract internal helpers to reduce branching.
- Update shell conditionals mechanically from `[` to `[[` with identical logic.
- Adjust visible labels or `aria-*` attributes so accessible name matches visible label text.

## Target Files
- `tests/e2e/test-webtor-http-through-tor.mjs`
- `tests/e2e/test-regression.mjs`
- `tests/e2e/test-demo-playwright.mjs`
- `tests/e2e/test-example.mjs`
- `tests/e2e/test-http-flow.mjs`
- `tests/e2e/test-tor-http.mjs`
- `webtor-demo/static/index.html`
- `build.sh`
- `scripts/fetch-consensus.sh`
- `example/build.sh`
- `tests/e2e/test_tor.sh`

## Acceptance Criteria
- SonarCloud shows 0 HIGH issues for the project.
- No behavior changes in tests or demo UI.
- CI remains green (or unchanged where flaky).

## Testing
- Run targeted e2e tests where feasible.
- Smoke-check demo UI for label changes.
