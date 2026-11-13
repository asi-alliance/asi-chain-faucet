# ASI Faucet Backend Server

Rust-based REST API service for distributing test ASI tokens on ASI blockchain. Built with Axum framework and interacts directly with ASI blockchain nodes via HTTP and gRPC APIs.

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

- Rust nightly selected by [rust-toolchain.toml](rust-toolchain.toml)
- Protocol Buffers Compiler: `apt install protobuf-compiler`

### Setup

1. **Fetch dependencies:**

   ```bash
   cargo fetch --locked
   ```

   The backend uses [asi-alliance/asi-chain-client, branch `rust-dev`](https://github.com/asi-alliance/asi-chain-client/tree/rust-dev)
   as a Cargo git dependency pinned to `5d1c9b4d12e33ec8b64a74a28df196ef03478e73`.
   Cargo fetches it automatically; `Cargo.lock` records the exact Git source.
   Node protocol dependencies remain at `0ccc315d84b32d668e8db49cd56ba3c576693e15`.
   Phlo prices must fit positive `i64` and are encoded as uint256 protobuf values.

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

### Upgrading an existing installation

Set `FAUCET_AMOUNT=10000` and `FAUCET_MAX_BALANCE=20000` explicitly in the
deployment configuration. Both use ASI tokens: the former atomic example
`FAUCET_AMOUNT=1000000000000` now means one trillion ASI, and the old ceiling
example `2000000000000` also means whole ASI. Explicitly set
`FAUCET_TOKEN_DECIMALS` to the chain's actual precision; its default changed
from 8 to 18, affecting deployments that set only `FAUCET_AMOUNT` as well.
Review all three values together and check the effective ASI and atomic
amounts in startup logs.

The observer must serve `/api/balance/{address}` and
`/api/deploy-finalization-status/{id}`. Missing finalization support is reported
as an error when querying deploy status. Keep
`FAUCET_CLAIMS_DIR` and `FAUCET_NETWORK_ID` stable across upgrades. See
[inspection and recovery](CONFIGURATION.md#inspecting-and-recovering-a-stuck-reservation)
for claims that remain pending.

---

## Architecture

### Module Organization

```
src/
├── main.rs              # Entry point, logging setup
├── config.rs            # Configuration management
├── utils.rs             # Node selection, validation
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
    ├── alerts.rs        # Mattermost notifications
    ├── claims.rs        # Durable per-recipient reservations
    └── node_cli.rs      # Blockchain interaction service
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
- Signed deploy submission via gRPC `do_deploy`

**Read-Only Observer Node** (read operations):
- Balance queries and status checks
- Default HTTP port: 40453
- Configured via OBSERVER_HOST, OBSERVER_GRPC_PORT, OBSERVER_HTTP_PORT
- Initial balances via gRPC exploratory queries; finalized balance reconciliation via HTTP `/api/balance/{address}`
- Deploy status via `GET /api/deploy-finalization-status/{id}` (HTTP)

---

## Documentation

### Detailed Documentation

- **[API.md](API.md)** - Complete API reference with all endpoints, request/response formats, error codes, and examples
- **[CONFIGURATION.md](CONFIGURATION.md)** - Detailed configuration guide with all environment variables, validation rules, and security best practices
- **[DEVELOPMENT.md](DEVELOPMENT.md)** - Development guide with setup instructions, code structure, building, testing, and troubleshooting

### Quick Reference

**API Endpoints:**
- `GET /config` - Public precision, atomic amounts and eligibility policy
- `POST /transfer` - Send tokens to address
- `GET /balance/:address` - Query address balance
- `GET /deploy/:deploy_id` - Check transaction status

**Key Configuration:**
```bash
PRIVATE_KEY=<required>
NODE_HOSTS=["192.168.1.10","192.168.1.11"]  # IP/hostname without port
NODE_GRPC_PORTS=[40412,40422]
NODE_HTTP_PORTS=[40413,40423]
OBSERVER_HOST=localhost        # default: localhost
OBSERVER_GRPC_PORT=40452
OBSERVER_HTTP_PORT=40453       # default: 40453
ALERTS_ENABLED=false           # default: false
MATTERMOST_WEBHOOK_URL=        # required when ALERTS_ENABLED=true
ALERT_ENVIRONMENT=unknown      # environment label shown in alerts
```

**Critical error alerts:** with `ALERTS_ENABLED=true`, the server posts to a Mattermost incoming webhook when no validator node is reachable or a transfer or deploy fails. See [CONFIGURATION.md](CONFIGURATION.md#critical-error-alerts) for the full variable list and webhook setup.

**Token precision:** Set `FAUCET_TOKEN_DECIMALS` to the chain's actual precision (default 18). The frontend reads precision and exact atomic limits from the backend's `/config` endpoint.

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
