Create Rust unit and integration tests in the `tests` directory in a subfolder 
corresponding to the src subfolder, eg tests for src/api go in tests/api

Do not put Rust tests inline with the code, put unit tests in the test tree (like we do for Java).

Do not use default values on traits (outside test-only code)

Generally, all state should be part of a struct, not global or thread local except in very special cases. Ask before using global or thraed local storage.

Always validate that a call which returns Option succeeds

When a parameter is in use, do not start the name with'_' as this is the unused parameter marker.

When running grep, exclude binaries (for performance)
Create Rust unit and integration tests in the `tests` directory in a subfolder
corresponding to the src subfolder, eg tests for src/api go in tests/api

Do not put Rust tests inline with the code, put unit tests in the test tree (like we do for Java).

Do not use default values on traits.

Always validate that a call which returns Option succeeds

When a parameter is in use, do not start the name with'_' as this is the unused parameter marker.

Mark all tests with `#[test]` and use a separate `#[cfg` parameter where needed, so that Intellij can see the tests

When running grep, exclude binaries (for performance)
Create any temp files (logging, etc) in tmp so that they are gitignored

Do Not use Trump Case in Comments, etc.

Before finishing a task:

- run the "unit" test target and verify all tests pass, no warnings were reported
- Ensure the tests, bingle_jsi, bingle-local and bingle_webserver trees compile
- Ensure all tests are referenced in Cargo.toml
