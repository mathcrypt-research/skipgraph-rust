# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust implementation of Skip Graphs - a distributed data structure for efficient P2P network operations. The project is structured as a library crate with core algorithms, local implementations, and testing infrastructure.

## Architecture

The codebase is organized into three main modules:

### Core Module (`src/core/`)
- **Model Types**: Core data types including `Identifier` (32-byte), `Address`, `MembershipVector`, and `Direction`
- **Node Trait**: Generic interface for skip graph nodes with methods for search, join operations
- **Lookup Tables**: `LookupTable` and `ArrayLookupTable` implementations for maintaining skip graph structure
- **Search System**: `IdentifierSearchRequest`/`IdentifierSearchResult` for node discovery operations

### Local Module (`src/local/`)
- **BaseNode**: Concrete implementation of the Node trait for local/in-memory skip graph operations
- Contains integration tests for search functionality

### Network Module (`src/network/`)
- **Message System**: `Message` struct with `Payload` enum for network communication
- **Traits**: `Network` and `MessageProcessor` interfaces for network abstraction
- **Mock Implementation**: `MockNetwork` and related testing infrastructure (test-only)

## Development Commands

### Testing
```bash
make test           # Run all tests
cargo test          # Direct cargo command
```

### Code Quality
```bash
make lint           # Run clippy with strict warnings
make install-tools  # Install clippy and rustfmt
cargo fmt          # Format code
```

### Dependencies
The project uses minimal dependencies focused on cryptography (`hex`, `bs58`), randomization (`rand`), error handling (`anyhow`), logging (`tracing`), and string utilities (`fixedstr`).

## Key Constants
- `IDENTIFIER_SIZE_BYTES: usize = 32` - Size of node identifiers and membership vectors
- `LOOKUP_TABLE_LEVELS` - Number of levels in the skip graph lookup table

## Testing Patterns
- Mock network infrastructure is available in `src/network/mock/` for testing distributed scenarios
- Test utilities and fixtures are provided in `src/core/testutil/`
- Integration tests focus on search operations and node behavior