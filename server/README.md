# ASI Faucet Backend Server

Rust-based REST API service for distributing test ASI tokens on ASI blockchain. Built with Axum framework and integrated with ASI blockchain nodes through forked F1r3fly node CLI.

For complete project documentation, see the [main README](../README.md) in the root directory.

---

## Table of Contents

1. [Quick Start](#quick-start)
2. [Architecture](#architecture)
3. [Documentation](#documentation)
4. [License](#license)

---

## Quick Start

### Prerequisites

- Rust 1.77 or higher
- Protocol Buffers Compiler: `apt install protobuf-compiler`
- Make and Perl: `apt install make perl`

### Setup

1. **Initialize submodules:**

   ```bash
   git submodule init
   git submodule update
   ```

   Keep `rust-client` submodule up-to-dated by running:
   ```bash
   git submodule update --init --remote -- server/rust-client 
   ```

2. **Configure environment:**

   ```bash
   cp .env.example .env
   # Edit .env with your configuration
   ```

3. **Run the server:**

   ```bash
   cargo run --release
   ```

The server starts on `http://0.0.0.0:40470` by default (from .env.example).

---

## Architecture

### Module Organization

```
src/
├── main.rs              # Entry point, logging setup
├── config.rs            # Configuration management
├── utils.rs             # Utility functions
│
├── core/                # Application core
│   ├── mod.rs
│   └── app.rs           # App state and lifecycle
│
├── api/                 # HTTP API layer
│   ├── mod.rs
│   ├── router.rs        # Route definitions
│   ├── models.rs        # Request/response models
│   ├── handlers/        # Endpoint handlers
│   │   ├── transfer.rs
│   │   ├── balance.rs
│   │   └── deploy.rs
│   └── middleware/      # Custom middleware
│       └── request_id.rs
│
└── services/            # Business logic
    ├── mod.rs
    └── node_cli.rs      # Blockchain interaction
```

### Request Flow

1. **Middleware Chain** - Request ID, CORS, Body limit, Timeout
2. **Handler** - Route-specific processing
3. **Service Layer** - Business logic and blockchain interaction
4. **Response** - JSON serialization and compression

### Node Communication

**Validator Nodes** (write operations):
- Multiple nodes for load balancing
- Random selection per request
- Configured via NODE_HOSTS, NODE_GRPC_PORTS, NODE_HTTP_PORTS

**Read-Only Observer Node** (read operations):
- Balance queries and status checks
- Default gRPC port: 40452, HTTP port: 40453
- Configured via READONLY_HOST, READONLY_GRPC_PORT, READONLY_HTTP_PORT
- Consistent read performance

---

## Documentation

### Detailed Documentation

- **[API.md](API.md)** - Complete API reference with all endpoints, request/response formats, error codes, and examples
- **[CONFIGURATION.md](CONFIGURATION.md)** - Detailed configuration guide with all environment variables, validation rules, and security best practices
- **[DEVELOPMENT.md](DEVELOPMENT.md)** - Development guide with setup instructions, code structure, building, testing, and troubleshooting

### Quick Reference

**API Endpoints:**
- `POST /transfer` - Send tokens to address
- `GET /balance/:address` - Query address balance
- `GET /deploy/:deploy_id` - Check transaction status

**Key Configuration:**
```bash
PRIVATE_KEY=<required>
NODE_HOSTS=["192.168.1.10","192.168.1.11"]  # IP/hostname without port
NODE_GRPC_PORTS=[40412,40422]
NODE_HTTP_PORTS=[40413,40423]
READONLY_HOST=localhost        # default: localhost
READONLY_GRPC_PORT=40452       # default: 40452
READONLY_HTTP_PORT=40453       # default: 40453
```

**Important:** The backend uses 10^8 as the token decimal conversion factor (hardcoded). Frontend should use `VITE_TOKEN_DECIMALS=8` to match.

**Development:**
```bash
# Auto-reload on changes
cargo watch -x run

# Debug logging
RUST_LOG=debug cargo run

# Format code
cargo fmt
```

**Docker:**
```bash
docker compose up -d
docker compose logs -f
docker compose down
```

---

## License

Licensed under the Apache License 2.0. See [LICENSE](../LICENSE) for details.

---

For complete project overview and frontend documentation, see the [main README](../README.md).
