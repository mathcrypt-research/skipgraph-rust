# Contributor Guide

## Testing Instructions

- Check the CI pipeline in `.github/workflows`.
- Fix failing tests before opening a pull request; `cargo test` must pass.
- Document new or changed items using Rust doc comments (`///`).
- Ensure new code is covered by tests; check with `cargo tarpaulin` or `cargo llvm-cov`.
- Add or update tests for any modified or new code.

## Code Style

- Run `cargo fmt` before committing.
- Follow Rust’s idioms and best practices.
- Use clear, descriptive names.
- Keep functions focused and concise.
- Comment complex logic clearly.
- Document public items using `///` and check with `cargo doc`.
- Commit title: `Short description (≤50 chars)` in imperative mood.
- PR title: same format; no description or labels—maintainers handle that.
- Explain new/updated tests with doc comments.
- Update documentation and `README.md` for any changes.
