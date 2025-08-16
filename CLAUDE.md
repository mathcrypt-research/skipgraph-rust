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

## Rust Design Patterns

### Shallow Cloning for Logic-Handling Structures

**Principle**: All structures that handle core application logic should be shallow-copyable using `Clone`, with reference counting and mutual exclusion handled internally and transparently.

**Implementation Pattern**:
```rust
// Use Arc<RwLock<T>> for shared mutable state
pub struct LogicStruct {
    inner: Arc<RwLock<InnerLogicStruct>>,
    span: Span,  // other fields clone normally
}

// Implement shallow Clone
impl Clone for LogicStruct {
    fn clone(&self) -> Self {
        // Shallow clone: cloned instances share the same underlying data via Arc
        LogicStruct {
            inner: Arc::clone(&self.inner),
            span: self.span.clone(),
        }
    }
}

// For trait objects, use clone_box pattern
pub trait LogicTrait {
    /// Creates a shallow copy of this instance.
    /// Implementations should ensure that cloned instances share the same underlying data
    /// (e.g., using Arc for shared ownership). Changes made through one instance should be
    /// visible in all cloned instances.
    fn clone_box(&self) -> Box<dyn LogicTrait>;
}

impl Clone for Box<dyn LogicTrait> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
```

**Reference Implementation**: See `LookupTable` trait and `ArrayLookupTable` struct in `src/core/lookup/`

**Benefits**:
- **Efficient**: Only Arc pointer is cloned, not underlying data
- **Thread-safe**: RwLock provides safe concurrent access
- **Flexible**: Works with trait objects via clone_box pattern
- **Clear semantics**: Clone indicates shared ownership
- **Encapsulated**: Reference counting/locking is internal implementation detail

**When to Apply**:
- Core business logic structures that need shared mutable state
- Components requiring concurrent access from multiple threads
- Trait objects that need cloning capability
- Systems where multiple handles to the same logical entity are needed

**Why Not Copy**: Copy requires all fields to be Copy, doesn't work with trait objects, and semantically implies independent data rather than shared ownership.