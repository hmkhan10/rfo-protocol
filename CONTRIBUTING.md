# Contributing to RFO Protocol

Thank you for your interest in contributing to RFO! This document provides guidelines for contributing to the project.

---

## Table of Contents

1. [Getting Started](#getting-started)
2. [Development Setup](#development-setup)
3. [Code Style](#code-style)
4. [Testing](#testing)
5. [Pull Request Process](#pull-request-process)
6. [Issue Guidelines](#issue-guidelines)
7. [Code of Conduct](#code-of-conduct)

---

## Getting Started

### Prerequisites

- Rust 1.82+
- Docker (for PostgreSQL)
- Git
- PostgreSQL client (optional, for direct DB access)

### Fork and Clone

```bash
# Fork on GitHub, then clone
git clone https://github.com/YOUR_USERNAME/rfo-protocol.git
cd rfo-protocol

# Add upstream remote
git remote add upstream https://github.com/hmkhan10/rfo-protocol.git
```

---

## Development Setup

### 1. Start PostgreSQL

```bash
docker run -d --name rfo-postgres-dev \
  -e POSTGRES_USER=rfo \
  -e POSTGRES_PASSWORD=dev_pass \
  -e POSTGRES_DB=rfo_protocol \
  -p 5432:5432 postgres:16-alpine
```

### 2. Set Environment

```bash
export RFO_SECRET_KEY=$(openssl rand -hex 32)
export DATABASE_URL="postgres://rfo:dev_pass@localhost/rfo_protocol"
export RFO_API_KEYS="test_key:$(openssl rand -hex 16)"
```

### 3. Build

```bash
cargo build
```

### 4. Run Tests

```bash
cargo test
```

### 5. Run Locally

```bash
cargo run
```

Server starts on `http://localhost:3000`.

---

## Code Style

### Rust Guidelines

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `rustfmt` for formatting
- Use `clippy` for linting

```bash
# Format
cargo fmt

# Lint
cargo clippy -- -D warnings
```

### Naming Conventions

| Item | Convention | Example |
|------|------------|---------|
| Types | PascalCase | `RfoClient`, `HandshakeResponse` |
| Functions | snake_case | `generate_site_id`, `compile_doc` |
| Constants | SCREAMING_SNAKE_CASE | `MAX_TOKEN_COUNT`, `CACHE_TTL` |
| Modules | snake_case | `server/handlers.rs` |
| Files | snake_case | `rfo_protocol.rs` |

### Error Handling

- Use `Result<T, E>` for fallible operations
- Define custom error types with `thiserror`
- Avoid `unwrap()` in production code

```rust
// Good
fn parse_content(html: &str) -> Result<ParsedContent, ParseError> {
    // ...
}

// Bad
fn parse_content(html: &str) -> ParsedContent {
    // Will panic on error
}
```

### Documentation

- Document public items with `///` comments
- Include examples for complex functions

```rust
/// Generate a deterministic site ID for a domain.
///
/// The site ID is an HMAC-SHA256 hash of `{domain}|{hour_window}`,
/// rotating hourly to prevent long-term tracking.
///
/// # Arguments
///
/// * `domain` - The domain to generate a site ID for
/// * `secret` - The secret key for HMAC
///
/// # Examples
///
/// ```
/// let site_id = generate_site_id("example.com", "secret_key");
/// assert_eq!(site_id.len(), 64);
/// ```
pub fn generate_site_id(domain: &str, secret: &str) -> String {
    // ...
}
```

---

## Testing

### Test Structure

```
tests/
├── integration.rs    # HTTP stack tests
├── security.rs       # Security tests
├── concurrency.rs    # Race condition tests
└── protocol.rs       # Protocol compliance tests
```

### Running Tests

```bash
# All tests
cargo test

# Specific suite
cargo test --test security
cargo test --test integration
cargo test --test concurrency
cargo test --test protocol

# With output
cargo test -- --nocapture
```

### Writing Tests

```rust
#[tokio::test]
async fn test_handshake_returns_valid_site_id() {
    let app = create_test_app().await;
    let response = app.post("/rfo/handshake")
        .json(&json!({
            "domain_url": "https://example.com",
            "coordinates": {},
            "requested_payload": "Mdoc",
            "nonce": "550e8400-e29b-41d4-a716-446655440000",
            "timestamp": 1700000000
        }))
        .await;

    assert_eq!(response.status(), 200);
    let body: Value = response.json().await;
    assert!(body["header"]["site_id"].as_str().unwrap().len() == 64);
}
```

### Test Naming

```rust
#[tokio::test]
async fn test_<function_name>_<scenario>() {
    // ...
}

// Examples:
async fn test_handshake_returns_valid_site_id()
async fn test_auth_rejects_missing_api_key()
async fn test_cache_returns_hit_on_second_request()
```

---

## Pull Request Process

### 1. Create a Branch

```bash
git checkout -b feature/my-new-feature
# or
git checkout -b fix/my-bugfix
```

### 2. Make Changes

- Write code following the style guide
- Add tests for new functionality
- Update documentation if needed

### 3. Run Checks

```bash
# Format
cargo fmt

# Lint
cargo clippy -- -D warnings

# Tests
cargo test

# Security audit
cargo audit
```

### 4. Commit

```bash
git add .
git commit -m "feat: add new feature description"
```

**Commit message format**:

```
<type>: <description>

[optional body]

[optional footer]
```

**Types**:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (no logic changes)
- `refactor`: Code refactoring
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

### 5. Push and Create PR

```bash
git push origin feature/my-new-feature
```

Then create a Pull Request on GitHub.

### PR Requirements

- [ ] All tests pass
- [ ] Clippy warnings resolved
- [ ] Code formatted with rustfmt
- [ ] Documentation updated (if applicable)
- [ ] Commit messages follow convention
- [ ] PR description explains changes

---

## Issue Guidelines

### Bug Reports

Include:
- Steps to reproduce
- Expected behavior
- Actual behavior
- Environment (OS, Rust version, Docker version)
- Relevant logs

### Feature Requests

Include:
- Problem statement
- Proposed solution
- Alternatives considered
- Implementation complexity estimate

### Good First Issues

Look for issues labeled `good-first-issue`. These are:
- Well-scoped
- Documented
- Suitable for newcomers

---

## Code of Conduct

### Our Pledge

We are committed to providing a welcoming and inclusive experience for everyone.

### Expected Behavior

- Use welcoming and inclusive language
- Be respectful of differing viewpoints
- Accept constructive criticism gracefully
- Focus on what is best for the community

### Unacceptable Behavior

- Harassment of any kind
- Trolling or insulting comments
- Publishing others' private information
- Other conduct that would be inappropriate in a professional setting

---

## Questions?

- Open a GitHub Discussion
- Join our community chat (link in README)
- Email: [your-email]

Thank you for contributing to RFO!
