.PHONEY install-lint:
install-lint:
	@echo "Installing clippy"
	@rustup update
	@rustup component add clippy

.PHONEY lint:
lint:
	@echo "Running clippy"
	@cargo clippy --all-targets --all-features -- -D warnings

.PHONY install-rustfmt:
install-rustfmt:
	@echo "Installing rustfmt"
	@rustup component add rustfmt

.PHONY install-tools:
install-tools: install-lint install-rustfmt
	@echo "✅ All tools installed"

.PHONEY test:
test:
	@echo "Running tests"
	@cargo test
.PHONY format:
format:
	@echo "Formatting code"
	@cargo fmt --all -- --check