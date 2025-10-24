# ASI Faucet Backend Server

Rust-based REST API service for distributing test REV tokens on ASI blockchain. Built with Axum framework and integrated with ASI blockchain nodes through forked F1r3fly node CLI.

For complete project documentation, see the [main README](../README.md) in the root directory.

---

## Table of Contents

1. [Quick Start](#quick-start)
2. [Architecture Details](#architecture-details)
   - [Module Organization](#module-organization)
   - [Request Processing Flow](#request-processing-flow)
   - [Node Communication](#node-communication)
3. [API Endpoints](#api-endpoints)
   - [POST /transfer](#post-transfer)
   - [GET /balance/:address](#get-balanceaddress)
   - [GET /deploy/:deploy_id](#get-deploydeploy_id)
4. [Configuration Reference](#configuration-reference)
   - [Environment Variables](#environment-variables)
   - [Configuration Validation](#configuration-validation)
5. [Middleware Stack](#middleware-stack)
6. [Error Handling](#error-handling)
7. [Logging and Monitoring](#logging-and-monitoring)
8. [Docker Deployment](#docker-deployment)
9. [Development](#development)
10. [Troubleshooting](#troubleshooting)
11. [Performance Considerations](#performance-considerations)
12. [Security Best Practices](#security-best-practices)
13. [Dependencies](#dependencies)
14. [License](#license)

---

## Quick Start

1. **Initialize submodules:**

   ```bash
   git submodule init
   git submodule update
   ```

2. **Configure environment:**

   ```bash
   cp .env.example .env
   # Edit .env with your node configurations and private key
   # See Configuration Reference section below for details
   ```

3. **Run the server:**

   ```bash
   cargo run --release
   ```

The server will start on `http://0.0.0.0:40470` by default.

---

## Architecture Details

### Module Organization

The backend is organized into functional modules following Rust best practices:

```
src/
├── main.rs              # Application entry point, logging setup
├── config.rs            # Environment configuration management
├── utils.rs             # Utility functions (node selection, etc.)
│
├── core/               # Application core layer
│   ├── mod.rs          # Module exports
│   └── app.rs          # Application state and runtime
│
├── api/                # HTTP API layer
│   ├── mod.rs          # API module exports
│   ├── router.rs       # Route definitions and middleware
│   ├── models.rs       # Request/response models
│   ├── handlers/       # Request handlers
│   │   ├── mod.rs
│   │   ├── transfer.rs   # Token transfer endpoint
│   │   ├── balance.rs    # Balance query endpoint
│   │   └── deploy.rs     # Deploy status endpoint
│   └── middleware/     # Custom middleware
│       ├── mod.rs
│       └── request_id.rs # Request ID tracking
│
└── services/           # Business logic layer
    ├── mod.rs
    └── node_cli.rs     # Blockchain interaction service
```

### Request Processing Flow

1. **Request Reception**: Axum router receives HTTP request
2. **Middleware Chain**: Request passes through middleware layers
   - Request ID assignment
   - CORS validation
   - Body size limit check
   - Timeout enforcement
3. **Handler Execution**: Route-specific handler processes request
4. **Service Layer**: Handler delegates to NodeCliService
5. **Blockchain Interaction**: Service communicates with ASI nodes via node_cli
6. **Response Formation**: Result serialized to JSON
7. **Middleware Chain (return)**: Response passes through compression layer
8. **Response Delivery**: Client receives response

### Node Communication

The server interacts with ASI blockchain through two types of nodes:

**Validator Nodes** - Used for write operations (token transfers)
- Multiple nodes configured for load balancing
- Random selection on each transfer request
- Communicates via gRPC (port 40451) and HTTP (port 40453)
- Requires private key for signing transactions

**Read-Only Observer Node** - Used for read operations
- Balance queries
- Deploy status checks
- Dedicated node for consistent read performance
- No private key required

---

## API Endpoints

### POST /transfer

Validates recipient address and balance, then initiates token transfer to blockchain.

**Request Validation:**
1. Address format check (must start with "1111", 50-54 chars, alphanumeric)
2. Balance query to read-only observer node
3. Balance limit enforcement (default: 20,000 REV)

**Transfer Process:**
1. Random validator node selection
2. Deploy creation with configured amount
3. Deploy ID generation and return

**Response Times:**
- Typical: 1-3 seconds
- Maximum: 7 seconds (enforced by timeout middleware)

---

### GET /balance/:address

Queries current balance from read-only observer node.

**Implementation:**
- Uses `wallet_balance_command` from node_cli
- Connects to observer node via gRPC
- Returns raw balance string (smallest unit)

**Response Format:**
```json
{
  "balance": "1500000000000"
}
```

Balance is in smallest unit where 1 REV = 10^8 or 10^9 units (configurable via `VITE_TOKEN_DECIMALS`).

---

### GET /deploy/:deploy_id

Polls deploy status from read-only observer node with configurable retry logic.

**Implementation:**
- Uses `check_deploy_status` from node_cli
- Connects to observer node via HTTP
- Retries based on `DEPLOY_MAX_WAIT_SEC` and `DEPLOY_CHECK_INTERVAL_SEC`

**Deploy States:**
- `succeeded` - Deploy included in block and executed successfully
- `failed` - Deploy included in block but execution failed
- `pending` - Deploy not yet included in block

---

## Configuration Reference

### Environment Variables

All configuration is loaded from environment variables defined in `.env` file.

#### Required Variables

| Variable | Type | Description |
|----------|------|-------------|
| PRIVATE_KEY | string | Private key for signing faucet transactions |
| NODE_HOSTS | JSON array | Validator node hostnames: `["host1","host2"]` |
| NODE_GRPC_PORTS | JSON array | Validator gRPC ports: `[40451,40451]` |
| NODE_HTTP_PORTS | JSON array | Validator HTTP ports: `[40453,40453]` |
| READONLY_HOST | string | Read-only observer node hostname |

#### Optional Variables with Defaults

| Variable | Default | Description |
|----------|---------|-------------|
| FAUCET_AMOUNT | 10000 | Amount to send per transfer (in smallest unit) |
| FAUCET_MAX_BALANCE | 20000 | Maximum balance for eligibility (in REV) |
| READONLY_GRPC_PORT | 40452 | Observer node gRPC port |
| READONLY_HTTP_PORT | 40453 | Observer node HTTP port |
| SERVER_HOST | 0.0.0.0 | Server bind address |
| SERVER_PORT | 40470 | Server listening port |
| DEPLOY_MAX_WAIT_SEC | 6 | Max seconds to wait for deploy status |
| DEPLOY_CHECK_INTERVAL_SEC | 2 | Seconds between deploy status checks |
| RUST_LOG | asi_faucet=info | Logging configuration |

### Configuration Validation

The application validates configuration on startup:

```rust
pub fn validate(&self) -> Result<(), Box<dyn Error>> {
    if self.private_key.is_none() {
        return Err("PRIVATE_KEY environment variable is required".into());
    }
    if self.faucet_amount == 0 {
        return Err("FAUCET_AMOUNT must be greater than 0".into());
    }
    // Additional validation...
    Ok(())
}
```

The application will exit with a clear error message if validation fails.

---

## Middleware Stack

### Request ID Middleware

Assigns a unique UUID to each request for tracing and debugging.

**Implementation:**
- Generates UUID v4 for each request
- Adds `X-Request-ID` header to response
- Useful for correlating logs across services

### CORS Middleware

Enables cross-origin requests from web interface.

**Configuration:**
- Allowed origins: `*` (any origin)
- Allowed methods: GET, POST, OPTIONS
- Allowed headers: Content-Type, Authorization
- Max age: 3600 seconds

### Body Limit Middleware

Prevents DoS attacks by limiting request body size.

**Configuration:**
- Maximum body size: 1MB (1024 * 1024 bytes)
- Applied to all POST endpoints

### Timeout Middleware

Prevents resource exhaustion from slow requests.

**Configuration:**
- Request timeout: 7 seconds
- Applies to entire request lifecycle
- Returns 408 Request Timeout on expiry

### Compression Middleware

Reduces bandwidth usage with response compression.

**Configuration:**
- Supports: gzip, brotli
- Automatic content negotiation based on `Accept-Encoding` header

---

## Error Handling

### Error Types

The application uses `anyhow::Result` for error propagation with context:

```rust
use anyhow::{Context, Result};

app.run()
    .await
    .context("Application runtime error")?;
```

### API Error Responses

All errors return JSON with consistent format:

```rust
pub struct ErrorResponse {
    pub error: String,
    pub details: Option<String>,
}
```

**Validation Error Example:**
```json
{
  "error": "Address balance exceeds faucet eligibility threshold",
  "details": null
}
```

**Transfer Error Example:**
```json
{
  "error": "FAUCET: Transfer failed",
  "details": "Insufficient funds in faucet wallet"
}
```

---

## Logging and Monitoring

### Logging Configuration

The application uses `tracing` for structured logging with configurable log levels:

```bash
RUST_LOG=asi_faucet=info,tower_http=debug,axum=debug
```

**Log Levels:**
- `error` - Critical errors requiring immediate attention
- `warn` - Warning conditions (e.g., balance limit exceeded)
- `info` - Informational messages (e.g., transfer initiated)
- `debug` - Detailed debugging information
- `trace` - Very detailed trace information

### Key Log Events

**Application Lifecycle:**
```
INFO Starting ASI Faucet service
INFO Configuration loaded
INFO Application built successfully
INFO Server starting on 0.0.0.0:40470
INFO Listening on 0.0.0.0:40470
```

**Transfer Operations:**
```
INFO FAUCET: Transfer request received for address: 1111...
INFO FAUCET: Transfer to 1111... deployed with id d1f2e3...
ERROR FAUCET: Transfer failed to 1111... with error Insufficient funds
```

**Balance Validations:**
```
WARN FAUCET: Address 1111... balance 2500000000000 exceeds faucet limit 2000000000000
```

### Health Monitoring

While the application doesn't expose a dedicated health endpoint, you can monitor:
- Server startup logs for configuration validation
- Request/response logs for API health
- Error logs for operational issues
- Application runtime for crashes

---

## Docker Deployment

### Using Docker Compose

The recommended way to deploy the backend:

```bash
# Start the service
docker-compose up -d

# View logs
docker-compose logs -f

# Stop the service
docker-compose down
```

### Docker Compose Configuration

```yaml
version: '3.8'

services:
  asi-chain-faucet-server:
    build: .
    image: asi-chain-faucet-server:latest
    env_file:
      - .env
    ports:
      - "${SERVER_PORT}:${SERVER_PORT}"
    restart: unless-stopped
```

### Dockerfile

```dockerfile
FROM rust:slim

WORKDIR faucet

COPY . .

RUN ["apt", "update"]
RUN ["apt", "upgrade", "--yes"]
RUN ["apt", "install", "--yes", "make", "perl", "protobuf-compiler"]

RUN ["cargo", "build"]

CMD ["cargo", "run"]
```

**Note:** For production deployments, consider using multi-stage builds to reduce image size and using `cargo build --release` for optimized binaries.

---

## Development

### Running in Development Mode

```bash
# With auto-reload
cargo watch -x run

# With specific log level
RUST_LOG=debug cargo run

# With custom port
SERVER_PORT=8080 cargo run
```

### Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_name
```

### Property-Based Testing

The project includes `proptest` for property-based testing:

```toml
[dev-dependencies]
proptest = "1.6"
```

### Code Quality

```bash
# Format code
cargo fmt

# Lint code
cargo clippy

# Check for common mistakes
cargo clippy -- -D warnings
```

---

## Troubleshooting

### Common Issues

**Issue: "PRIVATE_KEY environment variable is required"**
- Ensure `.env` file exists and contains `PRIVATE_KEY=your_key`
- Check that `.env` is in the same directory as Cargo.toml

**Issue: "Failed to bind to address"**
- Port already in use - change `SERVER_PORT` in `.env`
- Check firewall settings
- Ensure proper permissions for binding to port

**Issue: "NODE_HOSTS, NODE_GRPC_PORTS, and NODE_HTTP_PORTS must have the same length"**
- Verify all three arrays in `.env` have identical lengths
- Check array format: `["host1","host2"]` with proper quotes

**Issue: Transfer fails with "Insufficient funds"**
- Faucet wallet balance is too low
- Check faucet wallet balance on blockchain
- Top up the faucet wallet

**Issue: Balance queries timeout**
- Check `READONLY_HOST` connectivity
- Verify `READONLY_GRPC_PORT` is correct
- Ensure observer node is running and accessible

### Debug Mode

Enable verbose logging to diagnose issues:

```bash
RUST_LOG=asi_faucet=debug,tower_http=trace cargo run
```

---

## Performance Considerations

### Request Throughput

The server can handle concurrent requests efficiently thanks to:
- Async/await with Tokio runtime
- Non-blocking I/O operations
- Connection pooling for blockchain nodes

**Typical Performance:**
- Simple balance queries: 100-200ms
- Token transfers: 1-3 seconds (blockchain dependent)
- Deploy status checks: 200-500ms

### Resource Usage

**Memory:**
- Base: ~10MB
- Per request: ~100KB
- Recommended: 256MB minimum

**CPU:**
- Minimal during idle
- Spikes during cryptographic operations
- Recommended: 1 CPU core minimum

### Scaling Recommendations

For high-traffic deployments:
1. Run multiple server instances behind load balancer
2. Use dedicated database for rate limiting
3. Implement Redis cache for balance queries
4. Monitor and scale validator node pool
5. Consider geographic distribution of nodes

---

## Security Best Practices

### Private Key Management

- **Never commit** private keys to version control
- Store in environment variables or secrets management system
- Rotate keys periodically
- Monitor faucet wallet balance

### Network Security

- Deploy behind reverse proxy (nginx, Caddy)
- Enable HTTPS in production
- Configure firewall rules
- Restrict access to validator nodes

### Application Security

- Keep dependencies updated: `cargo update`
- Regular security audits: `cargo audit`
- Monitor CVE databases
- Implement rate limiting at infrastructure level

---

## Dependencies

### Core Dependencies

| Dependency | Version | Purpose |
|------------|---------|---------|
| tokio | 1.0 | Async runtime |
| axum | 0.7 | Web framework |
| tower | 0.4 | Service middleware |
| tower-http | 0.5 | HTTP middleware |
| serde | 1.0 | Serialization |
| serde_json | 1.0 | JSON support |
| tracing | 0.1 | Logging |
| anyhow | 1.0 | Error handling |
| node_cli | custom | Blockchain client |

### Build Dependencies

- Protocol Buffers compiler for gRPC
- Make for build automation
- Perl for build scripts

---

## License

Licensed under the Apache License 2.0. See [LICENSE](../LICENSE) for details.

---

For complete project documentation including frontend setup and architecture overview, see the [main README](../README.md).
