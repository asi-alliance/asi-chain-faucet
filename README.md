<div align="center">

# ASI Chain: Faucet

[![Status](https://img.shields.io/badge/Status-BETA-FFA500?style=for-the-badge)](https://github.com/asi-alliance/asi-chain-faucet)
[![Version](https://img.shields.io/badge/Version-0.1.0-A8E6A3?style=for-the-badge)](https://github.com/asi-alliance/asi-chain-faucet/releases)
[![License](https://img.shields.io/badge/License-Apache%202.0-1A1A1A?style=for-the-badge)](LICENSE)
[![Docs](https://img.shields.io/badge/Docs-Available-C4F0C1?style=for-the-badge)](https://docs.asichain.io)

<h3>A web-based faucet service for distributing test ASI tokens on the ASI blockchain testnet</h3>

Part of the [**Artificial Superintelligence Alliance**](https://superintelligence.io) ecosystem

*Uniting Fetch.ai, SingularityNET and CUDOS*

</div>

---

ASI Chain Faucet is a production-ready service that enables developers and users to receive test ASI tokens for ASI blockchain testnet. The service provides a user-friendly web interface for requesting tokens and tracking transaction status, with built-in balance validation to ensure fair distribution.

---

## Table of Contents

1. [Overview](#overview)
2. [Key Features](#key-features)
3. [Quick Start](#quick-start)
4. [Project Structure](#project-structure)
5. [Architecture](#architecture)
6. [Documentation](#documentation)
7. [Contributing](#contributing)
8. [License](#license)

---

## Overview

The ASI Chain Faucet consists of two main components working together to provide a seamless token distribution experience:

**Backend Server** - A Rust-based REST API service built with Axum that handles token transfers, balance checks, and transaction status queries. The server interacts directly with ASI blockchain nodes via HTTP and gRPC APIs, providing reliable interaction with the blockchain network.

**Frontend Application** - A React-based web interface that allows users to request tokens by entering their ASI address, check their current balance, and track the status of their token transfer transactions in real-time.

---

## Key Features

- **Token Distribution** - Send a configured amount of test ASI tokens to valid testnet addresses
- **Balance Validation** - Requires the recipient's pre-claim balance to be strictly below the eligibility ceiling
- **Transaction Tracking** - Monitor the status of token transfers using deploy IDs
- **Address Validation** - Validates ASI address format before processing requests
- **Multi-Node Support** - Selects a reachable validator for submission; balance and finalization queries use the observer
- **CORS Enabled** - Full CORS support for cross-origin requests from the web interface
- **Critical Error Alerts** - Optional Mattermost notifications for unreachable validators, submission errors and deploy failures, with per-event-type throttling

---

## Quick Start

### Prerequisites

**Backend:**
- Rustup with the pinned `nightly-2026-02-09` toolchain (`server/rust-toolchain.toml`)
- Protocol Buffers Compiler
- Make and Perl

**Frontend:**
- Node.js 18.x or higher
- npm 9.x or higher

### Running the Backend

The client submodule uses [asi-alliance/asi-chain-client, branch `rust-dev`](https://github.com/asi-alliance/asi-chain-client/tree/rust-dev),
pinned to `5d1c9b4d12e33ec8b64a74a28df196ef03478e73` (2026-11-11). Builds use
that exact gitlink; `branch = rust-dev` records the source branch. Phlo prices
must fit positive `i64` and are encoded as uint256 protobuf values.
For existing clones, run `git submodule sync -- server/rust-client` before
updating submodules.

This pin supplies the exact-integer API decoding and phlo-price signing used
by the target node. Use `git submodule update --init --recursive` to reproduce
it; `--remote` follows a moving branch and is not part of the build procedure.
Integration into the client's `dev` branch and a subsequent repin require a
separate client change and compatibility check.

```bash
# Clone the repository
git clone https://github.com/asi-alliance/asi-chain-faucet.git
cd asi-chain-faucet

# Initialize and update submodules (rust-client is pinned to a commit recorded by the superproject)
git submodule update --init --recursive

cd server

# Configure environment
cp .env.example .env
# Edit .env with your configuration

# Run the server
cargo run --release
```

Server starts on `http://0.0.0.0:40470` by default (from .env.example).

### Upgrading an existing backend

Before restarting, explicitly set `FAUCET_AMOUNT=10000` and
`FAUCET_MAX_BALANCE=20000` in the existing deployment configuration. Both now
use ASI tokens. The old atomic `FAUCET_AMOUNT=1000000000000` would request
one trillion ASI; the old example ceiling `2000000000000` also means whole
ASI. The default `FAUCET_TOKEN_DECIMALS` has changed from 8 to 18: explicitly
set it to the actual chain precision, even if the deployment previously set
only `FAUCET_AMOUNT`. Review all three settings together. Startup logs show
the effective amount and ceiling in ASI and atomic units.

The observer must serve `/api/balance/{address}` and
`/api/deploy-finalization-status/{id}`. Missing finalization support is reported
as an error when querying deploy status. Preserve `FAUCET_CLAIMS_DIR` and
`FAUCET_NETWORK_ID` across upgrades. Follow the
[reservation recovery procedure](server/CONFIGURATION.md#inspecting-and-recovering-a-stuck-reservation)
before changing any journal files.

### Running the Frontend

```bash
cd ../web

# Install dependencies
npm install

# Configure environment
# Create .env file with:
# VITE_BASE_URL=http://localhost:40470  # Default is http://localhost:3001 if not set
# VITE_USER_GUIDE_URL=https://docs.asichain.io/faucet/usage  # User guide link opened from the help icon
# Amount precision and eligibility come from the backend's GET /config endpoint.
# VITE_MAX_POLLS_MINUTES_COUNT=15  # How long the UI polls transfer status before giving up, in minutes (default is 15)

# Start development server
npm run dev
```

Frontend available at `http://localhost:5173`.

### Running the Frontend in Docker

The image builds with `VITE_BASE_URL=/api`, which takes precedence over a value
in `web/.env`. Nginx forwards `/api/` to `http://host.docker.internal:40470/`.
Run the backend with `SERVER_HOST=0.0.0.0` and `SERVER_PORT=40470` for this
configuration. The web Compose service sets
`extra_hosts: ["host.docker.internal:host-gateway"]` so the container can reach
the host on Linux.

From the `web/` directory, run `docker compose up --build -d`; the frontend is
then available at `http://localhost:3000`. `nginx.conf.template` is copied as a
static config, without environment substitution. Changing the proxy's backend
host or port requires editing that file and rebuilding the image.

---

## Project Structure

```
asi-chain-faucet/
├── server/                     # Rust backend service
│   ├── src/
│   │   ├── api/               # API layer (handlers, models, router)
│   │   ├── core/              # Application core
│   │   ├── services/          # Business logic (blockchain interaction)
│   │   ├── config.rs          # Configuration management
│   │   └── main.rs            # Application entry point
│   ├── Cargo.toml             # Rust dependencies
│   ├── Dockerfile             # Backend container image
│   ├── docker-compose.yml     # Backend deployment
│   ├── README.md              # Backend overview
│   ├── API.md                 # API reference
│   ├── CONFIGURATION.md       # Configuration guide
│   └── DEVELOPMENT.md         # Development guide
│
└── web/                        # React frontend application
    ├── src/
    │   ├── components/        # Reusable UI components
    │   ├── pages/             # Application pages
    │   ├── hooks/             # Custom React hooks
    │   ├── api/               # Backend API client
    │   └── utils/             # Utility functions
    ├── package.json           # Node.js dependencies
    └── vite.config.ts         # Build configuration
```

---

## Architecture

### System Components

```
┌──────────────────────────────────────────────────────────────────┐
│                         User Browser                             │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │         React Frontend (Vite + TypeScript)                  │ │
│  │  • Address Input & Validation                               │ │
│  │  • Balance Display                                          │ │
│  │  • Transaction Status Tracker                               │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                              │                                   │
│                    HTTPS/CORS│                                   │
└──────────────────────────────┼───────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│                    Backend Server (Rust)                         │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │                    API Layer (Axum)                         │ │
│  │  • POST /transfer        - Token distribution               │ │
│  │  • GET  /balance/:addr   - Balance queries                  │ │
│  │  • GET  /deploy/:id      - Transaction status               │ │
│  │                                                             │ │
│  │  Middleware: CORS, Compression, Timeout, Request ID         │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                              │                                   │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │              NodeCliService (Business Logic)                │ │
│  │  • Transfer funds with validation                           │ │
│  │  • Check recipient balance limits                           │ │
│  │  • Query transaction status                                 │ │
│  │  • Random node selection for load balancing                 │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                              │                                   │
│                    gRPC/HTTP │                                   │
└──────────────────────────────┼───────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│                    ASI Blockchain Network                        │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐            │
│  │  Validator   │  │  Validator   │  │  Validator   │            │
│  │    Node 1    │  │    Node 2    │  │    Node 3    │            │
│  │  (gRPC/HTTP) │  │  (gRPC/HTTP) │  │  (gRPC/HTTP) │            │
│  └──────────────┘  └──────────────┘  └──────────────┘            │
│                                                                  │
│  ┌──────────────┐                                                │
│  │  Read-Only   │                                                │
│  │   Observer   │  ← Used for balance & status queries           │
│  │  (gRPC/HTTP) │                                                │
│  └──────────────┘                                                │
└──────────────────────────────────────────────────────────────────┘
```

### Technology Stack

**Backend:**
- Rust (`nightly-2026-02-09`) - Pinned backend toolchain
- Axum 0.8 - Modern async web framework
- Tokio 1.0 - Asynchronous runtime
- Tower-HTTP 0.5 - HTTP middleware
- secp256k1 + blake2 - Cryptographic signing
- f1r3fly-models - Protobuf types for deploy signing

**Frontend:**
- React 19.1 - UI framework
- TypeScript 5.7 - Type-safe JavaScript
- Vite 6.2 - Fast build tool
- Redux Toolkit 2.6 - State management

**Infrastructure:**
- Docker - Containerization
- Docker Compose - Multi-container orchestration

See [server/Cargo.toml](server/Cargo.toml) and [web/package.json](web/package.json) for complete dependencies.

---

## Documentation

### Backend Documentation

**[server/README.md](server/README.md)** - Backend overview, quick start, architecture, and navigation

**[server/API.md](server/API.md)** - Complete API reference
- All endpoints (GET /config, POST /transfer, GET /balance, GET /deploy)
- Request/response formats
- Error codes and handling
- CORS configuration
- Usage examples with curl

**[server/CONFIGURATION.md](server/CONFIGURATION.md)** - Configuration guide
- All environment variables explained
- Required vs optional settings
- Validation rules
- Security best practices
- Docker deployment configuration

**[server/DEVELOPMENT.md](server/DEVELOPMENT.md)** - Development guide
- Development environment setup
- Running and debugging
- Code structure and patterns
- Building
- Troubleshooting common issues

### Additional Resources

- **[ASI Alliance Documentation](https://docs.asichain.io)** - General ASI blockchain documentation
- **[GitHub Repository](https://github.com/asi-alliance/asi-chain-faucet)** - Source code and issue tracking

---

## Contributing

Known issues and bugs are tracked in the [Issues](https://github.com/asi-alliance/asi-chain-faucet/issues) section.  
If you encounter a problem that is not listed, please feel free to open a new issue and provide as much detail as possible.

---

## License

This project is licensed under the Apache License 2.0. See [LICENSE](LICENSE) file for details.

---

ASI Alliance founding members: Fetch.ai, SingularityNET, and CUDOS
