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

### Internal Thread Safety Pattern

**Principle**: Prefer internal thread safety over external mutual exclusion. Structures should be internally thread-safe using Arc<RwLock<T>> patterns rather than requiring external Arc<Mutex<T>> wrapping.

**Preferred Pattern**:
```rust
// GOOD: Internal thread safety
pub struct ThreadSafeStruct {
    inner: Arc<RwLock<InnerState>>,
}

impl ThreadSafeStruct {
    pub fn method(&self) -> Result<()> {
        // Internal locking handled transparently
        self.inner.write().unwrap().do_something()
    }
}

// Usage: Simple Arc sharing
let instance = Arc<ThreadSafeStruct>::new(ThreadSafeStruct::new());
```

**Avoid Pattern**:
```rust
// AVOID: External mutual exclusion
pub struct UnsafeStruct {
    data: SomeData,
}

impl UnsafeStruct {
    pub fn method(&mut self) -> Result<()> {
        self.data.do_something()
    }
}

// Usage: Complex Arc<Mutex<>> wrapping required
let instance = Arc<Mutex<UnsafeStruct>>::new(UnsafeStruct::new());
let guard = instance.lock().unwrap();
guard.method()?;
```

**Benefits**:
- **Simplified Usage**: No external locking required by consumers
- **Encapsulation**: Thread safety is an implementation detail
- **Go-like**: Similar to Go's sync.Mutex pattern where structs handle their own locking
- **Cleaner APIs**: Methods take &self instead of &mut self when possible
- **Reduced Complexity**: Consumers don't need to manage Arc<Mutex<Option<T>>> patterns

**Reference Implementation**: See `MessageProcessor` struct in `src/network/mod.rs` - this wrapper type enforces internal thread-safety at the interface level. Developers implement the simple `MessageProcessorCore` trait, and the `MessageProcessor` wrapper automatically provides thread-safety, eliminating the need for `Arc<Mutex<Option<Box<dyn MessageProcessor>>>>` patterns.

### Struct-Level RwLock Pattern (Go-like Approach)

**Principle**: When a struct contains multiple fields that need coordinated mutation, avoid placing Mutex/RwLock on individual fields. Instead, group related fields into an Inner struct and govern the entire Inner struct with a single RwLock. This mirrors Go's approach where structs contain a single sync.RWMutex to protect all their fields.

**Preferred Pattern**:
```rust
// GOOD: Single RwLock governing entire inner state
pub struct ExternalStruct {
    core: Arc<RwLock<InnerExternalStruct>>,
}

struct InnerExternalStruct {
    hub: Arc<SomeComponent>,           // Components provide their own internal thread-safety
    processor: Arc<Option<SomeProcessor>>, // Immutable Arc once set
    other_field: String,
}

impl ExternalStruct {
    pub fn read_operation(&self) -> Result<String> {
        let core_guard = self.core.read()
            .map_err(|_| anyhow!("Failed to acquire read lock"))?;
        
        // Read access to all fields under single lock
        Ok(core_guard.other_field.clone())
    }
    
    pub fn write_operation(&self, processor: SomeProcessor) -> Result<()> {
        let mut core_guard = self.core.write()
            .map_err(|_| anyhow!("Failed to acquire write lock"))?;
        
        // Write access to coordinate changes across fields
        core_guard.processor = Arc::new(Some(processor));
        core_guard.other_field = "updated".to_string();
        Ok(())
    }
}
```

**Avoid Pattern**:
```rust
// AVOID: Multiple individual locks creating potential deadlocks
pub struct BadStruct {
    hub: Arc<RwLock<SomeComponent>>,        // Individual field locks
    processor: Arc<RwLock<Option<SomeProcessor>>>, // Can cause deadlocks
    other_field: Arc<RwLock<String>>,       // Complex lock coordination needed
}

impl BadStruct {
    pub fn complex_operation(&self) -> Result<()> {
        // Potential deadlock: multiple lock acquisition order matters
        let hub_guard = self.hub.write()?;
        let proc_guard = self.processor.write()?;  // Deadlock risk!
        let field_guard = self.other_field.write()?; // More deadlock risk!
        
        // Complex coordination logic here...
        Ok(())
    }
}
```

**Key Guidelines**:
1. **One RwLock Per Logical Entity**: Each struct should have one primary RwLock protecting its core state
2. **Inner Struct Pattern**: Group mutable fields into an Inner struct protected by the RwLock
3. **Component Thread-Safety**: Individual components (like `Arc<NetworkHub>`) handle their own internal thread-safety
4. **Immutable References**: Use `Arc<Option<T>>` for components that are set once and never mutated
5. **Read/Write Segregation**: Use read locks for access operations, write locks for mutations
6. **Lock Granularity**: Choose lock granularity based on logical operations, not individual field access

**Benefits**:
- **Deadlock Prevention**: Single lock eliminates complex lock ordering issues
- **Atomic Operations**: Related field changes happen atomically under one lock
- **Go-like Simplicity**: Mirrors familiar Go patterns with sync.RWMutex
- **Clear Ownership**: One lock clearly owns the struct's mutable state
- **Performance**: Fewer lock acquisitions for operations affecting multiple fields

**Reference Implementation**: See `MockNetwork` struct in `src/network/mock/network.rs` - uses `Arc<RwLock<InnerMockNetwork>>` pattern where `InnerMockNetwork` contains all mutable state, while individual components like `Arc<NetworkHub>` provide their own internal thread-safety.

## Code Style Guidelines

### Lowercase Error Messages and Logs

**Principle**: All error messages, log statements, and panic messages must be entirely lowercase to maintain consistency across the codebase.

**Examples**:
```rust
// GOOD: Lowercase error messages
return Err(anyhow!("failed to acquire read lock on core"));
tracing::error!("connection timeout occurred");
panic!("invalid state: expected Some but got None");

// BAD: Uppercase error messages  
return Err(anyhow!("Failed to acquire read lock on core"));
tracing::error!("Connection timeout occurred");
panic!("Invalid state: expected Some but got None");
```

**Applies to**:
- `anyhow!()` macro error messages
- `tracing::*!()` macro log messages (error, warn, info, debug, trace)
- `panic!()` macro messages
- `.expect()` and `.context()` strings
- Error messages in `Result::Err()` variants
- Test assertion messages

**Why Lowercase**:
- **Consistency**: Ensures uniform error reporting across all modules
- **Readability**: Lowercase messages are easier to scan in log files
- **Professional**: Avoids the appearance of "shouting" in error messages
- **Parsability**: Consistent casing makes log parsing more reliable

**Enforcement**: This style is mandatory for all new code and should be applied when modifying existing error handling.