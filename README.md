<div align="center">

# ASI Chain Faucet

[![Status](https://img.shields.io/badge/Status-Production--Ready-4A9D5F?style=for-the-badge)](https://github.com/asi-chain/asi-chain-faucet)
[![Version](https://img.shields.io/badge/Version-0.1.0-A8E6A3?style=for-the-badge)](https://github.com/asi-chain/asi-chain-faucet/releases)
[![License](https://img.shields.io/badge/License-Apache%202.0-1A1A1A?style=for-the-badge)](LICENSE)
[![Docs](https://img.shields.io/badge/Docs-Available-C4F0C1?style=for-the-badge)](https://docs.asichain.io)

<h3>A web-based faucet service for distributing test REV tokens on the ASI blockchain testnet</h3>

Part of the [**Artificial Superintelligence Alliance**](https://superintelligence.io) ecosystem

*Uniting Fetch.ai, SingularityNET, and CUDOS*

</div>

---

ASI Chain Faucet is a production-ready service that enables developers and users to receive test REV tokens for ASI blockchain testnet. The service provides a user-friendly web interface for requesting tokens and tracking transaction status, with built-in balance validation to ensure fair distribution.

---

## Table of Contents

1. [Overview](#overview)
   - [Key Features](#key-features)
   - [Use Cases](#use-cases)
2. [Project Structure](#project-structure)
   - [Key Directories](#key-directories)
3. [Architecture](#architecture)
   - [System Components](#system-components)
   - [Technology Stack](#technology-stack)
   - [Data Flow](#data-flow)
   - [Security Considerations](#security-considerations)
4. [Installation](#installation)
   - [Prerequisites](#prerequisites)
   - [Quick Start](#quick-start)
5. [Configuration](#configuration)
   - [Backend Configuration](#backend-configuration)
   - [Frontend Configuration](#frontend-configuration)
6. [API Reference](#api-reference)
   - [POST /transfer](#post-transfer)
   - [GET /balance/:address](#get-balanceaddress)
   - [GET /deploy/:deploy_id](#get-deploydeploy_id)
7. [Development](#development)
   - [Running Tests](#running-tests)
   - [Development Mode](#development-mode)
   - [Building for Production](#building-for-production)
8. [License](#license)

---

## Overview

The ASI Chain Faucet consists of two main components working together to provide a seamless token distribution experience:

**Backend Server** - A Rust-based REST API service built with Axum that handles token transfers, balance checks, and transaction status queries. The server integrates with ASI blockchain nodes through a forked F1r3fly node CLI, providing reliable interaction with the blockchain network.

**Frontend Application** - A React-based web interface that allows users to request tokens by entering their REV address, check their current balance, and track the status of their token transfer transactions in real-time.

### Key Features

- **Token Distribution**: Send a configured amount of test REV tokens to valid testnet addresses
- **Balance Validation**: Automatically checks recipient balance before transfer to prevent exceeding the faucet limit
- **Transaction Tracking**: Monitor the status of token transfers using deploy IDs
- **Address Validation**: Validates REV address format before processing requests
- **Multi-Node Support**: Load balances requests across multiple validator nodes for reliability
- **CORS Enabled**: Full CORS support for cross-origin requests from the web interface

### Use Cases

- Developers testing smart contracts on ASI blockchain testnet
- Users exploring ASI blockchain features without real funds
- Automated testing environments requiring test token provisioning
- Educational purposes for learning blockchain interactions

---

## Project Structure

The repository is organized as a monorepo containing both backend and frontend applications:

```
asi-chain-faucet/
├── server/                     # Rust backend service
│   ├── src/
│   │   ├── api/               # API layer
│   │   │   ├── handlers/      # Request handlers (transfer, balance, deploy)
│   │   │   ├── middleware/    # Request processing middleware
│   │   │   ├── models.rs      # API request/response models
│   │   │   └── router.rs      # API routing configuration
│   │   ├── core/              # Application core
│   │   │   ├── app.rs         # Application setup and runtime
│   │   │   └── mod.rs         # Core module exports
│   │   ├── services/          # Business logic
│   │   │   ├── node_cli.rs    # Blockchain interaction service
│   │   │   └── mod.rs         # Service module exports
│   │   ├── config.rs          # Configuration management
│   │   ├── utils.rs           # Utility functions
│   │   └── main.rs            # Application entry point
│   ├── rust-client/           # Forked F1r3fly node CLI (submodule)
│   ├── Cargo.toml             # Rust dependencies and metadata
│   ├── Dockerfile             # Backend container image
│   ├── docker-compose.yml     # Backend deployment configuration
│   ├── .env.example           # Environment variables template
│   └── README.md              # Backend-specific documentation
│
├── web/                        # React frontend application
│   ├── src/
│   │   ├── components/        # Reusable UI components
│   │   │   ├── Faucet/        # Main faucet component
│   │   │   ├── AddressBalance/# Balance display component
│   │   │   ├── TransactionStatusChecker/ # Deploy status checker
│   │   │   └── ...            # Other UI components
│   │   ├── pages/
│   │   │   └── Faucet/        # Main faucet page
│   │   ├── hooks/             # Custom React hooks
│   │   │   ├── useAddressInput.ts    # Address input management
│   │   │   ├── useDebounce.ts        # Input debouncing
│   │   │   └── useInputWithValidation.ts # Validation logic
│   │   ├── api/               # Backend API client
│   │   │   └── index.ts       # API methods (transfer, balance, status)
│   │   ├── utils/             # Utility functions
│   │   │   ├── config.ts      # Frontend configuration
│   │   │   └── ...
│   │   ├── context/           # React context providers
│   │   ├── layouts/           # Page layouts
│   │   └── index.tsx          # Application entry point
│   ├── package.json           # Node.js dependencies
│   ├── vite.config.ts         # Vite bundler configuration
│   ├── tsconfig.json          # TypeScript configuration
│   └── index.html             # HTML entry point
│
├── .github/
│   └── workflows/             # CI/CD deployment pipelines
├── Dockerfile                 # Frontend container image
└── .gitmodules                # Git submodules configuration
```

### Key Directories

**server/src/api** - Contains all HTTP API endpoints, request/response models, and middleware for processing incoming requests. The router module defines the API structure with CORS, compression, and timeout layers.

**server/src/services** - Business logic layer that interfaces with the ASI blockchain through the node CLI. Handles token transfers, balance queries, and transaction status checks.

**web/src/components** - Reusable React components including the main faucet form, balance display, transaction status checker, and various UI elements styled with custom CSS.

**web/src/hooks** - Custom React hooks for managing address input, debouncing user input, and implementing validation logic with real-time feedback.

For detailed backend server documentation, see [server/README.md](server/README.md).

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
│  │  • User Manual                                              │ │
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

**Backend Technologies**
- **Rust 1.77** - Systems programming language for performance and safety
- **Axum 0.7** - Modern async web framework built on Tokio
- **Tokio 1.0** - Asynchronous runtime for concurrent operations
- **Tower 0.4** - Middleware and service abstractions
- **Tower-HTTP 0.5** - HTTP middleware (CORS, compression, timeouts)
- **node_cli** - Forked F1r3fly CLI for blockchain interaction
- **Serde 1.0** - Serialization/deserialization framework
- **Tracing 0.1** - Structured logging and diagnostics

**Frontend Technologies**
- **React 19.1** - UI framework for building interactive interfaces
- **TypeScript 5.7** - Type-safe JavaScript for better development experience
- **Vite 6.2** - Fast build tool and development server
- **React Router 7.4** - Client-side routing
- **Redux Toolkit 2.6** - State management
- **Custom CSS** - Component-scoped styling

**Infrastructure**
- **Docker** - Containerization for consistent deployment
- **Docker Compose** - Multi-container orchestration

### Data Flow

**Token Transfer Flow**

1. User enters REV address in web interface
2. Frontend validates address format locally
3. Frontend sends POST request to `/transfer` endpoint
4. Backend validates address format on server side
5. Backend queries recipient balance from read-only observer node
6. Backend verifies balance is below faucet limit (20,000 REV)
7. Backend selects random validator node for load balancing
8. Backend initiates transfer using private key via node CLI
9. Backend returns deploy ID to frontend
10. Frontend displays deploy ID and provides status tracking link

**Balance Check Flow**

1. User enters REV address or receives it from faucet operation
2. Frontend sends GET request to `/balance/:address`
3. Backend validates address format
4. Backend queries balance from read-only observer node via node CLI
5. Backend returns balance as string representation
6. Frontend displays formatted balance in REV tokens

**Transaction Status Flow**

1. User enters deploy ID from previous transfer
2. Frontend sends GET request to `/deploy/:deploy_id`
3. Backend validates deploy ID format
4. Backend queries deploy status from read-only observer node
5. Backend returns status, message, and block hash if available
6. Frontend displays transaction outcome to user

### Security Considerations

- Private key stored in environment variables, never exposed to frontend
- Address validation on both frontend and backend to prevent invalid requests
- Balance limit enforcement to prevent abuse of faucet service
- CORS configuration to control allowed origins
- Request body size limits (1MB) to prevent DoS attacks
- Request timeout limits (7 seconds) for resource protection
- REV address format validation using blockchain-specific rules

---

## Installation

### Prerequisites

Before installing the ASI Chain Faucet, ensure you have the following installed:

**Backend Requirements**
- **Rust 1.77 or higher** - Install from [rust-lang.org](https://www.rust-lang.org/tools/install)
- **Protocol Buffers Compiler** - Required for gRPC: `apt install protobuf-compiler`
- **Make** - Build automation: `apt install make`
- **Perl** - Build dependency: `apt install perl`

**Frontend Requirements**
- **Node.js 18.x or higher** - Install from [nodejs.org](https://nodejs.org/)
- **npm 9.x or higher** - Comes with Node.js

**Optional (for Docker deployment)**
- **Docker 24.x or higher**
- **Docker Compose 2.x or higher**

### Quick Start

#### Backend Setup

1. Clone the repository with submodules:

```bash
git clone --recursive https://github.com/asi-chain/asi-chain-faucet.git
cd asi-chain-faucet/server
```

If you already cloned without `--recursive`, initialize submodules:

```bash
git submodule init
git submodule update
```

2. Configure environment variables:

```bash
cp .env.example .env
# Edit .env with your configuration (see Configuration section below)
```

3. Build and run the server:

```bash
cargo build --release
cargo run --release
```

The server will start on `http://0.0.0.0:40470` by default.

#### Frontend Setup

1. Navigate to the web directory:

```bash
cd ../web
```

2. Install dependencies:

```bash
npm install
```

3. Configure environment variables:

Create `.env` file in the `web` directory:

```bash
VITE_BASE_URL=http://localhost:40470
VITE_FAUCET_BALANCE_LIMIT=20000
VITE_TOKEN_DECIMALS=9
```

4. Start the development server:

```bash
npm run dev
```

The frontend will be available at `http://localhost:5173`.

#### Docker Deployment

For production deployment using Docker:

**Backend:**

```bash
cd server
docker-compose up -d
```

**Frontend:**

```bash
cd ..
docker build -t asi-chain-faucet-web:latest .
docker run -p 80:80 asi-chain-faucet-web:latest
```

---

## Configuration

### Backend Configuration

The backend server is configured through environment variables defined in `.env` file. All settings have default values but must be customized for your deployment.

See [.env.example](server/.env.example) for a complete template with all available configuration options.

#### Faucet Settings

```bash
# Amount of REV tokens to send per request (in smallest unit, 1 REV = 10^8 units)
FAUCET_AMOUNT=10000

# Maximum balance a recipient can have to be eligible (in REV)
FAUCET_MAX_BALANCE=20000

# Private key of the faucet wallet (required, no default)
PRIVATE_KEY=<your_private_key_here>
```

#### Node Configuration

Configure validator nodes for token transfers. All three arrays must have the same length:

```bash
# Validator node hostnames or IPs
NODE_HOSTS=["validator1.asi.io","validator2.asi.io","validator3.asi.io"]

# gRPC ports for each validator
NODE_GRPC_PORTS=[40451,40451,40451]

# HTTP ports for each validator
NODE_HTTP_PORTS=[40453,40453,40453]
```

#### Read-Only Observer Node

Used for balance queries and transaction status checks:

```bash
READONLY_HOST=observer.asi.io
READONLY_GRPC_PORT=40452
READONLY_HTTP_PORT=40453
```

#### Server Settings

```bash
# Server bind address (use 0.0.0.0 to accept external connections)
SERVER_HOST=0.0.0.0

# Server port
SERVER_PORT=40470
```

#### Deploy Status Checking

Configuration for transaction status polling:

```bash
# Maximum time to wait for deploy status (seconds)
DEPLOY_MAX_WAIT_SEC=6

# Interval between status checks (seconds)
DEPLOY_CHECK_INTERVAL_SEC=2
```

#### Logging

```bash
# Logging level configuration
# Format: module=level,module=level
RUST_LOG=asi_faucet=info,tower_http=debug
```

### Frontend Configuration

The frontend is configured through environment variables in `.env` file or at build time.

Create a `.env` file in the `web/` directory with the following variables:

```bash
# Backend API URL (no trailing slash)
VITE_BASE_URL=https://faucet-api.asi.io

# Maximum balance limit for faucet eligibility (in REV)
VITE_FAUCET_BALANCE_LIMIT=20000

# Token decimal places
VITE_TOKEN_DECIMALS=9
```

### Configuration Validation

The backend validates configuration on startup and will exit with an error if:
- `PRIVATE_KEY` is not set
- `FAUCET_AMOUNT` is zero or negative
- `NODE_HOSTS`, `NODE_GRPC_PORTS`, and `NODE_HTTP_PORTS` have different lengths
- Any required environment variable is missing

---

## API Reference

The backend exposes a RESTful API with JSON responses. All endpoints support CORS and include standard HTTP middleware (compression, timeouts).

### Base URL

```
http://localhost:40470
```

### Endpoints

#### POST /transfer

Transfers test REV tokens to a specified address.

**Request**

```http
POST /transfer HTTP/1.1
Content-Type: application/json

{
  "to_address": "11114GuXVLzHJqUqDUJGLJJsn8c1234567890abcdefghijklmnopqrst"
}
```

**Request Body Parameters**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| to_address | string | Yes | Valid REV address starting with "1111" (50-54 characters) |

**Response**

Success (200 OK):
```json
{
  "deploy_id": "d1f2e3b4a5c6789012345678901234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef12"
}
```

Error (400 Bad Request):
```json
{
  "error": "Address balance exceeds faucet eligibility threshold",
  "details": null
}
```

**Validation Rules**

- Address must start with "1111"
- Address length must be between 50 and 54 characters
- Address must contain only alphanumeric characters
- Recipient balance must be below 20,000 REV (configurable)
- Server must have valid `PRIVATE_KEY` configured

**Status Codes**

- `200 OK` - Transfer successfully initiated
- `400 Bad Request` - Invalid address format or balance exceeds limit
- `500 Internal Server Error` - Server error during transfer

---

#### GET /balance/:address

Retrieves the current balance of a REV address.

**Request**

```http
GET /balance/11114GuXVLzHJqUqDUJGLJJsn8c1234567890abcdefghijklmnopqrst HTTP/1.1
```

**Path Parameters**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| address | string | Yes | Valid REV address to query |

**Response**

Success (200 OK):
```json
{
  "balance": "1500000000000"
}
```

The balance is returned as a string representing the smallest token unit (1 REV = 10^8 or 10^9 units depending on configuration).

Error (400 Bad Request):
```json
{
  "error": "Invalid address format",
  "details": "Address must start with 1111"
}
```

**Status Codes**

- `200 OK` - Balance retrieved successfully
- `400 Bad Request` - Invalid address format
- `500 Internal Server Error` - Error querying blockchain

---

#### GET /deploy/:deploy_id

Retrieves the status and information about a deploy transaction.

**Request**

```http
GET /deploy/d1f2e3b4a5c6789012345678901234567890abcdef1234567890abcdef12 HTTP/1.1
```

**Path Parameters**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| deploy_id | string | Yes | Deploy ID returned from /transfer endpoint (100-160 characters) |

**Response**

Success (200 OK):
```json
{
  "status": "succeeded",
  "msg": "Transfer completed successfully",
  "block_hash": "a1b2c3d4e5f6789012345678901234567890abcdef"
}
```

Pending:
```json
{
  "status": "pending",
  "msg": "Deploy is still being processed"
}
```

Failed:
```json
{
  "status": "failed",
  "msg": "Insufficient funds",
  "block_hash": null
}
```

**Response Fields**

| Field | Type | Description |
|-------|------|-------------|
| status | string | Deploy status: "succeeded", "failed", or "pending" |
| msg | string | Human-readable status message |
| block_hash | string\|null | Block hash if deploy is included in a block, null otherwise |

**Status Codes**

- `200 OK` - Deploy status retrieved successfully
- `400 Bad Request` - Invalid deploy ID format
- `404 Not Found` - Deploy ID not found
- `500 Internal Server Error` - Error querying blockchain

---

### Error Response Format

All error responses follow a consistent format:

```json
{
  "error": "Brief error description",
  "details": "Additional error context (optional)"
}
```

### Rate Limiting

Currently, no rate limiting is implemented in the API layer. Rate limiting should be configured at the infrastructure level (e.g., reverse proxy, API gateway) for production deployments.

### CORS Configuration

The API accepts requests from any origin (`*`) with the following:
- **Allowed Methods**: GET, POST, OPTIONS
- **Allowed Headers**: Content-Type, Authorization
- **Max Age**: 3600 seconds

---

## Development

### Running Tests

**Backend:**

```bash
cd server
cargo test
```

**Frontend:**

```bash
cd web
npm run lint
```

### Development Mode

**Backend with hot reload:**

```bash
cd server
cargo watch -x run
```

**Frontend with hot module replacement:**

```bash
cd web
npm run dev
```

### Building for Production

**Backend:**

```bash
cd server
cargo build --release
```

The optimized binary will be in `target/release/asi-faucet`.

**Frontend:**

```bash
cd web
npm run build
```

The production build will be in `dist/` directory.

### Code Structure Best Practices

- **Backend**: Follow Rust conventions with explicit error handling using `Result` and `anyhow`
- **Frontend**: Use TypeScript strict mode, follow React hooks patterns, implement custom hooks for reusable logic
- **API Communication**: All API calls should include proper error handling and loading states
- **Validation**: Implement validation on both client and server side for security

---

## License

This project is licensed under the Apache License 2.0. See [LICENSE](LICENSE) file for details.

---

ASI Alliance founding members: Fetch.ai, SingularityNET, and CUDOS
