Create Rust unit and integration tests in the `tests` directory in a subfolder 
corresponding to the src subfolder, eg tests for src/api go in tests/api

Do not put Rust tests inline with the code, put unit tests in the test tree (like we do for Java).

Always validate that a call which returns Option succeeds

Before finishing a task:

- Ensure the tests tree compiles
- Ensure all tests are referenced in Cargo.toml
- run `cargo test` to make sure all tests pass.
