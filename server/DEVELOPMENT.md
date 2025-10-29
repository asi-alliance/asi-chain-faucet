# Development Guide

Complete development guide for ASI Chain Faucet backend server.

---

## Table of Contents

1. [Getting Started](#getting-started)
2. [Development Environment](#development-environment)
3. [Running the Server](#running-the-server)
4. [Code Structure](#code-structure)
5. [Building](#building)
6. [Troubleshooting](#troubleshooting)

---

## Getting Started

### Prerequisites

Install required tools before starting development:

**Rust Toolchain:**
```bash
# Install Rust (includes cargo)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify installation
rustc --version
cargo --version
```

**System Dependencies:**
```bash
# Ubuntu/Debian
sudo apt update
sudo apt install -y protobuf-compiler make perl

# macOS
brew install protobuf make
```

**Optional Development Tools:**
```bash
# Auto-reload on file changes
cargo install cargo-watch

# Code formatting
rustup component add rustfmt

# Linting
rustup component add clippy
```

### Initial Setup

1. **Clone repository with submodules:**

```bash
git clone --recursive https://github.com/asi-alliance/asi-chain-faucet.git
cd asi-chain-faucet/server
```

If already cloned without `--recursive`:
```bash
git submodule init
git submodule update
```

2. **Create configuration file:**

```bash
cp .env.example .env
```

3. **Edit `.env` with your development settings:**

```bash
# Minimal development config
PRIVATE_KEY=<your_test_private_key>
NODE_HOSTS=["http://testnet-node1:26657"]
NODE_GRPC_PORTS=[40412]
NODE_HTTP_PORTS=[40413]
READONLY_HOST=testnet-observer.asi.io
RUST_LOG=asi_faucet=debug,tower_http=debug
```

---

## Development Environment

### Environment Variables for Development

Create a `.env` file with development-specific settings:

```bash
# Development Configuration
FAUCET_AMOUNT=100000
FAUCET_MAX_BALANCE=50000
PRIVATE_KEY=<dev_private_key>

# Local or testnet nodes
NODE_HOSTS=["http://localhost:26657"]
NODE_GRPC_PORTS=[40412]
NODE_HTTP_PORTS=[40413]
READONLY_HOST=localhost
READONLY_GRPC_PORT=40452
READONLY_HTTP_PORT=40453

# Development server settings
SERVER_HOST=127.0.0.1
SERVER_PORT=40470

# Verbose logging
RUST_LOG=asi_faucet=debug,tower_http=debug,axum=debug

# Relaxed deploy checking
DEPLOY_MAX_WAIT_SEC=10
DEPLOY_CHECK_INTERVAL_SEC=2
```

---

## Running the Server

### Development Mode

**Basic run:**
```bash
cargo run
```

**With release optimizations:**
```bash
cargo run --release
```

**With auto-reload (requires cargo-watch):**
```bash
cargo watch -x run
```

**With specific log level:**
```bash
RUST_LOG=debug cargo run
```

**With custom port:**
```bash
SERVER_PORT=8080 cargo run
```

### Development Workflow

1. **Start the server:**
```bash
cargo watch -x run
```

2. **In another terminal, test endpoints:**
```bash
# Health check
curl http://localhost:40470/balance/11114GuXVLzHJqUqDUJGLJJsn8c1234567890abcdefghijklmnopqrst

# Request tokens
curl -X POST http://localhost:40470/transfer \
  -H "Content-Type: application/json" \
  -d '{"to_address":"11114GuXVLzHJqUqDUJGLJJsn8c1234567890abcdefghijklmnopqrst"}'
```

3. **Make code changes** - server auto-reloads with cargo-watch

---

## Code Structure

### Module Organization

```
src/
├── main.rs              # Entry point, logging setup
├── config.rs            # Configuration loading and validation
├── utils.rs             # Utility functions
│
├── core/                # Application core
│   ├── mod.rs
│   └── app.rs           # App state and lifecycle
│
├── api/                 # HTTP API layer
│   ├── mod.rs
│   ├── router.rs        # Route definitions
│   ├── models.rs        # Request/response types
│   ├── handlers/        # Endpoint handlers
│   │   ├── mod.rs
│   │   ├── transfer.rs
│   │   ├── balance.rs
│   │   └── deploy.rs
│   └── middleware/      # Custom middleware
│       ├── mod.rs
│       └── request_id.rs
│
└── services/            # Business logic
    ├── mod.rs
    └── node_cli.rs      # Blockchain interaction
```

### Adding New Endpoints

1. **Define request/response models in `api/models.rs`:**

```rust
#[derive(Debug, Deserialize)]
pub struct MyRequest {
    pub field: String,
}

#[derive(Debug, Serialize)]
pub struct MyResponse {
    pub result: String,
}
```

2. **Create handler in `api/handlers/`:**

```rust
use crate::api::models::{ApiResult, MyRequest, MyResponse};
use axum::{extract::State, Json};

pub async fn my_handler(
    State(state): State<AppState>,
    Json(request): Json<MyRequest>,
) -> ApiResult<MyResponse> {
    // Implementation
    Ok(Json(MyResponse {
        result: "success".to_string(),
    }))
}
```

3. **Register route in `api/router.rs`:**

```rust
use crate::api::handlers::my_handler;

pub fn create_router(app_state: AppState) -> Router {
    Router::new()
        .route("/my-endpoint", post(my_handler))
        // ... existing routes
        .with_state(app_state)
}
```

### Adding New Services

1. **Create service module in `services/`:**

```rust
// services/my_service.rs
pub struct MyService {
    config: AppConfig,
}

impl MyService {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }
    
    pub async fn do_something(&self) -> Result<String> {
        // Implementation
        Ok("result".to_string())
    }
}
```

2. **Export in `services/mod.rs`:**

```rust
pub mod my_service;
pub use my_service::MyService;
```

3. **Use in handlers:**

```rust
let service = MyService::new(state.config.clone());
let result = service.do_something().await?;
```

---

## Building

### Development Build

```bash
cargo build
```

Output: `target/debug/asi-faucet`

### Release Build

```bash
cargo build --release
```

Output: `target/release/asi-faucet`

**Optimizations:**
- Smaller binary size
- Faster execution
- Longer compile time
- No debug symbols

### Build for Specific Target

```bash
# List available targets
rustc --print target-list

# Build for specific target
cargo build --release --target x86_64-unknown-linux-musl
```

### Docker Build

```bash
docker build -t asi-faucet-server:latest .
```




## Troubleshooting

### Common Development Issues

#### "error: linker `cc` not found"

**Solution:**
```bash
# Ubuntu/Debian
sudo apt install build-essential

# macOS
xcode-select --install
```

#### "protoc not found"

**Solution:**
```bash
# Ubuntu/Debian
sudo apt install protobuf-compiler

# macOS
brew install protobuf
```

#### "submodule `rust-client` is not initialized"

**Solution:**
```bash
git submodule init
git submodule update
```

#### "error: failed to compile openssl-sys"

**Solution:**
```bash
# Ubuntu/Debian
sudo apt install pkg-config libssl-dev

# macOS
brew install openssl
export OPENSSL_DIR=$(brew --prefix openssl)
```

#### Server starts but endpoints return 404

**Check:**
1. Routes are registered in `api/router.rs`
2. Server is listening on expected port
3. Request path matches route definition
4. HTTP method (GET/POST) matches

#### "Address already in use" error

**Solution:**
```bash
# Find process using port
lsof -i :40470

# Kill process
kill -9 <PID>

# Or change SERVER_PORT in .env
SERVER_PORT=8080
```

#### "Failed to connect to node"

**Check:**
1. Node is running and accessible
2. Firewall allows connection
3. Correct host and port in `.env`
4. Network connectivity

```bash
# Test connection
nc -zv <node_host> <port>

# Or use curl for HTTP
curl http://<node_host>:<http_port>
```

#### "PRIVATE_KEY environment variable is required"

**Solution:**
```bash
# Ensure .env file exists
ls -la .env

# Check PRIVATE_KEY is defined
grep PRIVATE_KEY .env

# Verify .env is in correct directory (server/)
pwd  # Should be .../asi-chain-faucet/server
```

### Performance Issues

#### Slow compilation

**Solutions:**
1. Use incremental compilation (enabled by default in dev)
2. Reduce optimization level in dev
3. Use `cargo-chef` for Docker builds
4. Add to `~/.cargo/config.toml`:
```toml
[build]
incremental = true
```

#### High memory usage

**Solutions:**
1. Build with `--release` flag
2. Reduce `DEPLOY_MAX_WAIT_SEC` to decrease concurrent operations
3. Implement connection pooling for nodes
4. Monitor with:
```bash
cargo build --release
/usr/bin/time -v target/release/asi-faucet
```

### Debugging Network Issues

**Test node connectivity:**
```bash
# gRPC port
grpcurl -plaintext <node_host>:<grpc_port> list

# HTTP port
curl -v http://<node_host>:<http_port>
```

**Capture HTTP traffic:**
```bash
# Using tcpdump
sudo tcpdump -i any -A 'tcp port 40470'

# Using wireshark
wireshark
```

**Test API endpoints:**
```bash
# Using curl with verbose output
curl -v -X POST http://localhost:40470/transfer \
  -H "Content-Type: application/json" \
  -d '{"to_address":"11114GuXVLzHJqUqDUJGLJJsn8c1234567890abcdefghijklmnopqrst"}'
```

### Getting Help

If issues persist:

1. Check logs with `RUST_LOG=debug`
2. Review [API.md](API.md) for endpoint details
3. Review [CONFIGURATION.md](CONFIGURATION.md) for config details
4. Search existing GitHub issues
5. Create new issue with:
   - Rust version (`rustc --version`)
   - OS and version
   - Configuration (with sensitive data removed)
   - Full error message
   - Steps to reproduce

---

For API documentation, see [API.md](API.md).  
For configuration details, see [CONFIGURATION.md](CONFIGURATION.md).
