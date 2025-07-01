# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is **Jito-Solana**, a production fork of the Solana blockchain validator that adds MEV (Maximum Extractable Value) functionality. It's a complex Rust-based blockchain validator implementation with strict performance and quality requirements.

## Build and Development Commands

**Primary Commands:**
- `./cargo build` - Debug build (uses custom wrapper with proper toolchain)
- `./cargo build --release` - Production release build  
- `./cargo test` - Run test suite
- `./cargo bench` - Run benchmarks (requires nightly Rust)

**Code Quality:**
- `scripts/cargo-fmt.sh` - Format code using rustfmt
- `scripts/cargo-clippy.sh` - Run Clippy linter
- `scripts/coverage.sh` - Generate code coverage reports
- `scripts/cargo-install-all.sh` - Install all tools and dependencies

**Testing Requirements:**
- Minimum 90% code coverage for new code paths
- Benchmark all performance-critical changes with evidence posted to PRs
- Use `cargo +nightly bench` for benchmarking

## Architecture Overview

**Core Validator Components:**
- `core/` - Main validator logic, banking stage, consensus mechanisms
- `runtime/` - Bank implementation, account processing, transaction execution
- `ledger/` - Blockchain data storage, retrieval, and persistence
- `gossip/` - P2P networking and cluster communication protocols
- `rpc/` - JSON-RPC API server for client interactions
- `streamer/` - Network packet processing and I/O

**Jito MEV-Specific Components:**
- `bundle/` - Transaction bundle processing for MEV extraction
- `bundle-sdk/` - SDK for bundle operations and client interactions
- `tip-distributor/` - MEV tip distribution logic and mechanisms
- `jito-protos/` - Protocol buffer definitions for Jito-specific APIs
- `jito-programs/` - On-chain Solana programs (git submodule)

**Supporting Infrastructure:**
- `accounts-db/` - Account storage, caching, and database operations
- `svm/` - Solana Virtual Machine implementation
- `programs/` - Built-in system programs (stake, vote, system, etc.)
- `perf/` - Performance optimization utilities and measurements

## Development Standards

**Code Quality Requirements:**
- Use `rustfmt` with imports_granularity = "One" configuration
- Run Clippy with custom rules (9 argument threshold, specific disallowed methods)
- Prefer `.expect()` over `.unwrap()` with descriptive error messages
- No `unwrap()` without proof of safety in comments

**Naming Conventions:**
- Functions: `<verb>_<subject>` pattern (e.g., `process_transaction`)
- Variables: lowercase with underscores, avoid abbreviations except in closures
- Types: UpperCamelCase, lowercase type names with underscores for variants

**Performance Standards:**
- All consensus or performance-critical changes must include benchmark data
- Use feature gates for consensus-breaking changes
- Profile and optimize hot code paths in banking/runtime components

**PR Guidelines:**
- Keep PRs small and focused (max ~1,000 lines for functional changes)
- Draft PRs encouraged for early feedback on architecture
- All changes reviewed by subject matter experts
- Include test coverage metrics for new functionality

## Key Technical Context

**MEV Architecture:**
- Bundle processing happens in banking stage alongside normal transactions
- Tip distribution uses on-chain programs with off-chain coordination
- Auction mechanisms determine bundle inclusion and ordering
- Integration points with Solana's existing transaction processing pipeline

**Build System:**
- Cargo workspace with 50+ crates
- Custom `./cargo` wrapper manages Rust toolchain (1.86.0)
- LTO optimization enabled for release builds
- Cross-platform CI/CD with performance regression detection

**Testing Strategy:**
- Unit tests for individual components
- Integration tests for end-to-end workflows
- Stress testing for performance-critical paths
- Benchmark suite for consensus and banking components