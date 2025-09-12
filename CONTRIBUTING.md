### Note on CI/CD

We use GitHub Actions pipelines to lint code, run checks, and automate builds.

Every merge request (MR) triggers a pipeline that must pass successfully. The most common reasons for a pipeline failure are:

* Formatting: Please run `cargo +nightly fmt`

* Linting: Please run `cargo clippy --all-targets -- -D warnings`

* Tests: Please run `cargo test --lib`

Merging with a failed pipeline is strongly discouraged and should only be done in exceptional circumstances, with a full understanding of the implications.