# Skip Graph (Rust)

An open-source implementation of Skip Graphs middleware written in Rust.

## Overview

[Skip Graph](https://arxiv.org/pdf/cs.DS/0306043) is a distributed data structure designed to achieve efficient search, insert, and delete operations
over a dynamic peer-to-peer
network.
This project provides a Rust-based implementation of Skip Graph middleware, suitable for building distributed systems requiring a P2P routing
overlay or distributed key-value store.

## Prerequisites

To use or contribute to this project, you need to have the following installed:

- **Rust**: Ensure you have Rust installed. You can download it from [Rust's official website](https://rust-lang.org/). The minimum required
  version is `rustc 1.88.0 (6b00bc388 2025-06-23)`.
- **Cargo**: Cargo is the Rust package manager and is included with the Rust installation. The minimum required version is `cargo 1.88.0 (873a06493 2025-05-10)`.
  .

## Getting Started

Follow these steps to set up and start working on the project.

### Cloning the Repository

Start by cloning this repository to your local machine:

```shell script
git clone github.com/thep2p/skipgraph-rust.git
cd skipgraph-rust
```

### Installing Dependencies

Run the following command to install required tools and dependencies:

```shell script
make install-tools
```

This command will:

- Install and update Rust, if necessary.
- Install the `clippy` component for linting.

## Usage

### Running the Tests

To ensure the project runs properly, execute the test suite using:

```shell script
make test
```

This command runs the `cargo test` command, executing all available unit tests to verify the implementation.

### Linting

To check the code for common issues and adhere to best practices, use the following command:

```shell script
make lint
```

This command runs `cargo clippy`, a linting tool for Rust projects.

## Development

### Adding New Tools

If you want to extend the tooling provided by this project, add new tasks to the `Makefile` and group them under appropriate targets.

### Contributing

Contributions to this project are welcome! Feel free to fork the repository, make your changes, and submit a pull request. Ensure you follow these
steps before submitting:

1. Run the tests using `make test`.
2. Lint the code using `make lint`.

By following these steps, you'll help maintain the code quality and robustness of the project.

## License

This project is under Apache 2.0 License. See the [LICENSE](LICENSE) file for more details.

