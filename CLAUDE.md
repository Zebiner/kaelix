# MemoryStreamer Development Guide
## Project Overview
MemoryStreamer is an ultra-high-performance distributed streaming system designed to revolutionize message streaming infrastructure with unprecedented speed and reliability.

### Performance Targets
- **Latency**: <10μs P99 end-to-end
- **Throughput**: 10M+ messages/second
- **Concurrent Streams**: 1M+ supported
- **Memory Efficiency**: <1KB per inactive stream
- **Reliability**: 99.99% uptime target

## TODO: Master Development Plan

### Phase 1: Foundation and Core Infrastructure

#### Completed Milestones
- [x] Plugin System Foundation Implementation (Week 3, Days 3-4)
  - Ultra-high-performance plugin infrastructure
  - Zero-overhead plugin invocation
  - Enterprise-grade security isolation
  - Comprehensive lifecycle management

#### Current Focus
- Phase 1, Week 3, Day 5: Telemetry & observability framework
- Phase 1, Week 3, Day 6: Stream multiplexing enhancement
- Phase 1, Week 3, Day 7: Phase 1 completion validation and Phase 2 preparation

### Phase 2: Advanced Streaming Capabilities (Upcoming)
- Distributed message routing
- High-availability clustering
- Advanced stream processing
- Enhanced security models

## Rust Code Quality Guidelines

### Core Principles
1. **Zero-tolerance for warnings**: ALL cargo check warnings must be resolved
2. **Perfect formatting**: 100% cargo fmt compliance required
3. **Comprehensive testing**: 95%+ test coverage mandatory
4. **Performance-first**: Every component optimized for ultra-high performance
5. **Security-conscious**: Memory safety and security by design
6. **Documentation-complete**: 100% public API documentation coverage

## Quality Enforcement: Zero-Tolerance Framework

### Compilation Rules: Absolute Correctness
- **ZERO TOLERANCE** for compilation errors AND warnings
- ALL `cargo check --workspace` warnings must be treated as blocking errors
- ALL `cargo clippy --workspace -- -D warnings` warnings must be treated as blocking errors
- Developers MUST resolve ALL warnings before task completion
- Use `cargo check` instead of `cargo build` for enhanced developer experience
- No exceptions, no partial merges

```bash
# Enhanced zero-tolerance enforcement
cargo check --workspace          # Must pass with ZERO warnings
cargo clippy --workspace -- -D warnings  # Must pass with ZERO warnings
cargo fmt --all --check         # Must pass with ZERO violations
cargo audit                     # Must pass with ZERO vulnerabilities
```

## Mandatory Testing Protocol: Completion-Driven Quality Gates

### Task Completion Testing Requirements
- **Code Changes**: Run `cargo check --workspace` and `cargo clippy --workspace -- -D warnings`
- **New Features**: Execute `cargo test --workspace` with coverage validation
- **Bug Fixes**: Run regression tests and affected module tests
- **Refactoring**: Execute full test suite to ensure no behavioral changes
- **Dependencies**: Run `cargo audit` for security vulnerability checks

### Quality Gate Enforcement
```bash
# MANDATORY commands at every completion:
cargo check --workspace                    # Zero warnings required
cargo clippy --workspace -- -D warnings    # Zero violations required
cargo fmt --all --check                    # Perfect formatting required
cargo test --workspace                     # All tests must pass
cargo audit                                # Zero vulnerabilities required
```

## Automated Weekly Documentation Protocol

### Multi-Agent Documentation Workflow
1. **reader-agent**: Analyze all code changes, implementations, and technical achievements
2. **plan-agent**: Document strategic decisions, architectural choices, and rationale
3. **docs-agent**: Create comprehensive weekly summary documentation
4. **test-agent**: Validate quality metrics and compliance achievements
5. **security-agent**: Document security considerations and vulnerability assessments

### Weekly Summary Structure
```markdown
# Week [X] Summary: [Phase Name] - [Week Description]

## Executive Summary
- Objectives achieved vs. planned
- Key deliverables and their status
- Critical milestones reached
- Quality compliance metrics

## Technical Achievements
### Code Implementation Details
- Files created/modified with line counts
- Key functions and data structures implemented
- Integration points and API changes
- Performance characteristics achieved
```

## CI/CD Enforcement Mechanisms
- Automated checks on EVERY pull request
- Blocking merge conditions:
  1. Zero compilation warnings
  2. 100% formatting compliance
  3. All tests passing
  4. Performance benchmarks within defined thresholds

## Development Workflow Integration
- Update CLAUDE.md when updating the todo list and vice versa
- Maintain comprehensive documentation for all architectural decisions
- Follow multi-agent coordination patterns for complex tasks
- Ensure every completion meets mandatory testing requirements

## Global Rules for Code Generation
- When generating code involving external libraries, always invoke Context7 MCP server first
- Structure responses as: 1) Plan with Context7 data, 2) List assumptions, 3) Generate code, 4) Explain changes

## Important Instruction Reminders
- Do what has been asked; nothing more, nothing less
- NEVER create files unless absolutely necessary
- ALWAYS prefer editing existing files
- NEVER proactively create documentation files unless explicitly requested