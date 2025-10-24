<div align="center">

# ASI Chain Faucet

[![Status](https://img.shields.io/badge/Status-Production--Ready-4A9D5F?style=for-the-badge)](https://github.com/asi-alliance/asi-chain-faucet)
[![Version](https://img.shields.io/badge/Version-0.1.0-A8E6A3?style=for-the-badge)](https://github.com/asi-alliance/asi-chain-faucet/releases)
[![License](https://img.shields.io/badge/License-Apache%202.0-1A1A1A?style=for-the-badge)](LICENSE)
[![Docs](https://img.shields.io/badge/Docs-Available-C4F0C1?style=for-the-badge)](https://docs.asichain.io)

<h3>A web-based faucet service for distributing test ASI tokens on the ASI blockchain testnet</h3>

Part of the [**Artificial Superintelligence Alliance**](https://superintelligence.io) ecosystem

*Uniting Fetch.ai, SingularityNET, and CUDOS*

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
7. [License](#license)

---

## Overview

The ASI Chain Faucet consists of two main components working together to provide a seamless token distribution experience:

**Backend Server** - A Rust-based REST API service built with Axum that handles token transfers, balance checks, and transaction status queries. The server integrates with ASI blockchain nodes through a forked F1r3fly node CLI, providing reliable interaction with the blockchain network.

**Frontend Application** - A React-based web interface that allows users to request tokens by entering their ASI address, check their current balance, and track the status of their token transfer transactions in real-time.

---

## Key Features

- **Token Distribution** - Send a configured amount of test ASI tokens to valid testnet addresses
- **Balance Validation** - Automatically checks recipient balance before transfer to prevent exceeding the faucet limit
- **Transaction Tracking** - Monitor the status of token transfers using deploy IDs
- **Address Validation** - Validates ASI address format before processing requests
- **Multi-Node Support** - Load balances requests across multiple validator nodes for reliability
- **CORS Enabled** - Full CORS support for cross-origin requests from the web interface

---

## Quick Start

### Prerequisites

**Backend:**
- Rust 1.77 or higher
- Protocol Buffers Compiler
- Make and Perl

**Frontend:**
- Node.js 18.x or higher
- npm 9.x or higher

### Running the Backend

```bash
# Clone with submodules
git clone --recursive https://github.com/asi-alliance/asi-chain-faucet.git
cd asi-chain-faucet/server

# Configure environment
cp .env.example .env
# Edit .env with your configuration

# Run the server
cargo run --release
```

Server starts on `http://0.0.0.0:40470` by default.

### Running the Frontend

```bash
cd ../web

# Install dependencies
npm install

# Configure environment
# Create .env file with:
# VITE_BASE_URL=http://localhost:40470  # Or http://localhost:3001 (default if not set)
# VITE_FAUCET_BALANCE_LIMIT=20000
# VITE_TOKEN_DECIMALS=9

# Start development server
npm run dev
```

Frontend available at `http://localhost:5173`.

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
│   ├── rust-client/           # Forked F1r3fly node CLI (submodule)
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
- Rust 1.77 - Systems programming language
- Axum 0.7 - Modern async web framework
- Tokio 1.0 - Asynchronous runtime
- Tower-HTTP 0.5 - HTTP middleware
- node_cli - Forked F1r3fly CLI for blockchain interaction

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
- All endpoints (POST /transfer, GET /balance, GET /deploy)
- Request/response formats
- Error codes and handling
- CORS configuration
- Usage examples with curl

**[server/CONFIGURATION.md](server/CONFIGURATION.md)** - Configuration guide
- All environment variables explained
- Required vs optional settings
- Validation rules
- Security best practices
- Docker and Kubernetes configuration

**[server/DEVELOPMENT.md](server/DEVELOPMENT.md)** - Development guide
- Development environment setup
- Running and debugging
- Code structure and patterns
- Building and testing
- Troubleshooting common issues

### Additional Resources

- **[ASI Alliance Documentation](https://docs.asichain.io)** - General ASI blockchain documentation
- **[GitHub Repository](https://github.com/asi-alliance/asi-chain-faucet)** - Source code and issue tracking

---

## License

This project is licensed under the Apache License 2.0. See [LICENSE](LICENSE) file for details.

---

ASI Alliance founding members: Fetch.ai, SingularityNET, and CUDOS
