# Configuration Reference

Complete configuration guide for ASI Chain Faucet backend server.

---

## Overview

The backend server is configured through environment variables defined in a `.env` file. All configuration is loaded at startup and validated before the server starts accepting requests.

---

## Environment Variables

### Required Variables

These variables **must** be set for the server to start:

#### PRIVATE_KEY

```bash
PRIVATE_KEY=<your_private_key_here>
```

**Description:** Private key of the faucet wallet used to sign all token transfer transactions.

**Format:** Hexadecimal string

**Security:**
- Never commit this to version control
- Store in secure secrets management system in production
- Rotate periodically
- Monitor faucet wallet balance

---

#### NODE_HOSTS

```bash
NODE_HOSTS=["192.168.1.10","192.168.1.11","192.168.1.12"]
```

**Description:** Array of validator node hostnames or IP addresses used for token transfers.

**Format:** JSON array of strings

**Requirements:**
- Must have same length as `NODE_GRPC_PORTS` and `NODE_HTTP_PORTS`
- At least one node must be specified
- All nodes should be accessible from the server
- This is a REQUIRED variable with no default value

**Example:**
```bash
NODE_HOSTS=["validator1.example.com","validator2.example.com"]
# or with IP addresses:
NODE_HOSTS=["192.168.1.10","192.168.1.11","192.168.1.12"]
```

---

#### NODE_GRPC_PORTS

```bash
NODE_GRPC_PORTS=[40412,40422,40432]
```

**Description:** Array of gRPC port numbers for each validator node.

**Format:** JSON array of integers

**Requirements:**
- Must have same length as `NODE_HOSTS` and `NODE_HTTP_PORTS`
- Each port corresponds to the node at the same index in `NODE_HOSTS`
- This is a REQUIRED variable with no default value

**Example:**
```bash
NODE_GRPC_PORTS=[40412,40422,40432]
```

---

#### NODE_HTTP_PORTS

```bash
NODE_HTTP_PORTS=[40413,40423,40433]
```

**Description:** Array of HTTP port numbers for each validator node.

**Format:** JSON array of integers

**Requirements:**
- Must have same length as `NODE_HOSTS` and `NODE_GRPC_PORTS`
- Each port corresponds to the node at the same index in `NODE_HOSTS`
- This is a REQUIRED variable with no default value

**Example:**
```bash
NODE_HTTP_PORTS=[40413,40423,40433]
```

---

#### READONLY_HOST

```bash
READONLY_HOST=observer.example.com
```

**Description:** Hostname or IP address of the read-only observer node used for balance queries and deploy status checks.

**Format:** String (hostname or IP)

**Default:** localhost

**Purpose:**
- Balance queries
- Transaction status checks
- Does not perform writes
- Reduces load on validator nodes

**Example:**
```bash
READONLY_HOST=localhost
# or with custom host:
READONLY_HOST=observer.example.com
# or with IP:
READONLY_HOST=192.168.1.20
```

---

### Optional Variables

These variables have default values and can be omitted:

#### FAUCET_AMOUNT

```bash
FAUCET_AMOUNT=10000
```

**Description:** Amount of ASI tokens to send per transfer request.

**Format:** Integer (positive, non-zero)

**Unit:** Smallest token unit (motes). In this implementation, 1 ASI = 10^8 motes (8 decimal places)

**Default:** 10000

**Validation:** Must be greater than 0

**Example:**
```bash
FAUCET_AMOUNT=10000  # Send 10,000 smallest units per request
```

---

#### FAUCET_MAX_BALANCE

```bash
FAUCET_MAX_BALANCE=20000
```

**Description:** Maximum balance (in ASI) a recipient can have to be eligible for faucet tokens.

**Format:** Integer (positive)

**Unit:** ASI tokens (not smallest unit)

**Default:** 20000

**Purpose:** Prevents abuse by limiting how many times the same address can receive tokens

**Calculation:** Balance is converted to smallest unit (motes) before comparison: `max_balance_allowed = FAUCET_MAX_BALANCE * 10^8`

**Important:** The backend uses a hardcoded conversion factor of 10^8 (not configurable). This means the token has 8 decimal places in the backend logic.

**Example:**
```bash
FAUCET_MAX_BALANCE=20000  # Addresses with 20,000+ ASI are ineligible
```

---

#### READONLY_GRPC_PORT

```bash
READONLY_GRPC_PORT=40452
```

**Description:** gRPC port of the read-only observer node.

**Format:** Integer (1-65535)

**Default:** 40452

**Used For:**
- Balance queries via gRPC

**Example:**
```bash
READONLY_GRPC_PORT=40452
```

---

#### READONLY_HTTP_PORT

```bash
READONLY_HTTP_PORT=40453
```

**Description:** HTTP port of the read-only observer node.

**Format:** Integer (1-65535)

**Default:** 40453

**Used For:**
- Deploy status queries via HTTP

**Example:**
```bash
READONLY_HTTP_PORT=40453
```

---

#### SERVER_HOST

```bash
SERVER_HOST=0.0.0.0
```

**Description:** IP address the server should bind to.

**Format:** IP address string

**Default:** 0.0.0.0

**Options:**
- `0.0.0.0` - Listen on all network interfaces (recommended for production)
- `127.0.0.1` - Listen only on localhost (for local development)
- Specific IP - Listen on specific network interface

**Example:**
```bash
SERVER_HOST=0.0.0.0  # Accept connections from any network
```

---

#### SERVER_PORT

```bash
SERVER_PORT=40470
```

**Description:** Port the server should listen on.

**Format:** Integer (1-65535)

**Default:** 8000

**Note:** The `.env.example` file uses 40470, but if this variable is not set, the server defaults to port 8000.

**Recommendation:** Use ports above 1024 to avoid requiring root privileges

**Example:**
```bash
SERVER_PORT=8080
```

---

#### DEPLOY_MAX_WAIT_SEC

```bash
DEPLOY_MAX_WAIT_SEC=6
```

**Description:** Maximum time (in seconds) to wait for deploy status when querying.

**Format:** Integer (positive)

**Default:** 6

**Purpose:** Controls how long the server will poll for deploy status before giving up

**Calculation:** Maximum attempts = `DEPLOY_MAX_WAIT_SEC / DEPLOY_CHECK_INTERVAL_SEC`

**Example:**
```bash
DEPLOY_MAX_WAIT_SEC=10  # Wait up to 10 seconds
```

---

#### DEPLOY_CHECK_INTERVAL_SEC

```bash
DEPLOY_CHECK_INTERVAL_SEC=2
```

**Description:** Interval (in seconds) between deploy status check attempts.

**Format:** Integer (positive)

**Default:** 2

**Purpose:** Controls polling frequency when checking deploy status

**Calculation:** Number of attempts = `DEPLOY_MAX_WAIT_SEC / DEPLOY_CHECK_INTERVAL_SEC`

**Example:**
```bash
DEPLOY_CHECK_INTERVAL_SEC=1  # Check every second
```

---

#### RUST_LOG

```bash
RUST_LOG=asi_faucet=info,tower_http=debug
```

**Description:** Logging level configuration for different components.

**Format:** Comma-separated list of `module=level` pairs

**Default:** asi_faucet=info

**Log Levels (from least to most verbose):**
- `error` - Critical errors only
- `warn` - Warnings and errors
- `info` - Informational messages
- `debug` - Detailed debugging information
- `trace` - Very detailed trace information

**Common Configurations:**

Production (minimal logging):
```bash
RUST_LOG=asi_faucet=info,tower_http=warn
```

Development (detailed logging):
```bash
RUST_LOG=asi_faucet=debug,tower_http=debug,axum=debug
```

Debugging (maximum verbosity):
```bash
RUST_LOG=asi_faucet=trace,tower_http=trace
```

---

## Token Decimals and Frontend Integration

### Backend Token Decimals

The backend uses a **hardcoded conversion factor of 10^8** for all balance calculations. This is implemented in `src/api/handlers/transfer.rs`:

```rust
let max_balance_allowed: u128 = state.config.faucet_max_balance as u128 * 10u128.pow(8);
```

This means:
- 1 ASI token = 10^8 motes (smallest unit)
- The token effectively has **8 decimal places**
- This value is **not configurable** via environment variables

### Frontend Configuration

The frontend uses `VITE_TOKEN_DECIMALS` environment variable to display balances correctly. To match the backend's behavior:

```bash
VITE_TOKEN_DECIMALS=8  # Must match backend's 10^8 conversion factor
```

**Warning:** If the frontend uses a different decimals value (e.g., the default of 9), balance displays will be incorrect by a factor of 10.

### Consistency Requirements

| Component | Setting | Value | Required |
|-----------|---------|-------|----------|
| Backend | Hardcoded in code | 10^8 | Fixed |
| Frontend | VITE_TOKEN_DECIMALS | 8 | Must match |
| Display | Human-readable | X.XXXXXXXX | 8 decimal places |

---

## Configuration File

### .env File Structure

Create a `.env` file in the `server/` directory:

```bash
# Faucet Configuration
FAUCET_AMOUNT=10000
FAUCET_MAX_BALANCE=20000
PRIVATE_KEY=<your_private_key>

# Validator Nodes
NODE_HOSTS=["<ENTER_NODE1_HOST>","<ENTER_NODE2_HOST>","<ENTER_NODE3_HOST>"]
NODE_GRPC_PORTS=[<ENTER_NODE1_GRPC_PORT>,<ENTER_NODE2_GRPC_PORT>,<ENTER_NODE3_GRPC_PORT>]
NODE_HTTP_PORTS=[<ENTER_NODE1_HTTP_PORT>,<ENTER_NODE2_HTTP_PORT>,<ENTER_NODE3_HTTP_PORT>]

# Read-Only Observer Node
READONLY_HOST=<READONLY_HOST>
READONLY_GRPC_PORT=40452
READONLY_HTTP_PORT=40453

# Server Configuration
SERVER_HOST=0.0.0.0
SERVER_PORT=40470

# Deploy Status Checking
DEPLOY_MAX_WAIT_SEC=6
DEPLOY_CHECK_INTERVAL_SEC=2

# Logging
RUST_LOG=asi_faucet=info,tower_http=debug
```

### .env.example Template

The repository includes a `.env.example` template:

```bash
cp .env.example .env
# Edit .env with your actual values
```

---

## Configuration Validation

The server validates all configuration on startup:

### Validation Rules

1. **Required Variables:**
   - `PRIVATE_KEY` must be set
   - `NODE_HOSTS` must be set and non-empty
   - `NODE_GRPC_PORTS` must be set and non-empty
   - `NODE_HTTP_PORTS` must be set and non-empty

2. **Array Length Consistency:**
   - `NODE_HOSTS`, `NODE_GRPC_PORTS`, and `NODE_HTTP_PORTS` must have the same length

3. **Value Constraints:**
   - `FAUCET_AMOUNT` must be greater than 0
   - Port numbers must be valid (1-65535)

### Validation Errors

If validation fails, the server will exit with a clear error message:

```
Error: PRIVATE_KEY environment variable is required
```

```
Error: NODE_HOSTS, NODE_GRPC_PORTS, and NODE_HTTP_PORTS must have the same length
```

```
Error: FAUCET_AMOUNT must be greater than 0
```

---

## Security Best Practices

### Private Key Management

**DO:**
- Store in environment variables or secrets manager
- Use separate keys for different environments (dev/staging/prod)
- Rotate keys periodically
- Monitor faucet wallet balance
- Set up alerts for low balance

**DON'T:**
- Commit to version control
- Share via unencrypted channels
- Use the same key across environments
- Store in plaintext files

---

## Environment-Specific Configuration

### Docker Deployment

When using Docker, pass environment variables through:

**docker-compose.yml:**
```yaml
services:
  faucet:
    env_file:
      - .env
```

**docker run:**
```bash
docker run --env-file .env asi-faucet-server
```



## Troubleshooting Configuration

### Common Issues

**"PRIVATE_KEY environment variable is required"**
- Ensure `.env` file exists in `server/` directory
- Check that `PRIVATE_KEY` is defined in `.env`
- Verify no typos in variable name

**"NODE_HOSTS, NODE_GRPC_PORTS, and NODE_HTTP_PORTS must have the same length"**
- Count elements in each array
- Ensure all arrays have identical lengths
- Check JSON array format: `["item1","item2"]`

**"Failed to parse NODE_HOSTS"**
- Verify JSON array format
- Check for missing quotes around strings
- Ensure proper comma separation

**Server fails to start with "Address already in use"**
- Another process is using `SERVER_PORT`
- Change `SERVER_PORT` to a different value
- Check for other faucet instances running

### Verification Commands

**Check if .env is loaded:**
```bash
cd server
cargo run 2>&1 | grep "Configuration loaded"
```

**Verify node connectivity:**
```bash
# Test gRPC port (replace with your node details)
nc -zv <your_node_host> <your_grpc_port>

# Test HTTP port (replace with your node details)
curl http://<your_node_host>:<your_http_port>
```

**Test configuration:**
```bash
# Set minimal config for testing
cat > .env << EOF
PRIVATE_KEY=test_key_for_validation
NODE_HOSTS=["localhost"]
NODE_GRPC_PORTS=[40451]
NODE_HTTP_PORTS=[40453]
READONLY_HOST=localhost
EOF

# Run server (will fail to connect but validates config)
cargo run
```

---

For API documentation, see [API.md](API.md).  
For development information, see [DEVELOPMENT.md](DEVELOPMENT.md).
