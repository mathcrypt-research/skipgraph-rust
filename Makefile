.PHONEY install-lint:
install-lint:
	@echo "Installing clippy"
	@rustup update
	@rustup component add clippy

.PHONEY lint:
lint:
	@echo "Running clippy"
	@cargo clippy

.PHONEY install-tools:
install-tools: install-lint

.PHONEY test:
test:
	@echo "Running tests"
	@cargo test