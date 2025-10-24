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
NODE_HOSTS=["validator1.asi.io","validator2.asi.io","validator3.asi.io"]
```

**Description:** Array of validator node hostnames or IP addresses used for token transfers.

**Format:** JSON array of strings

**Requirements:**
- Must have same length as `NODE_GRPC_PORTS` and `NODE_HTTP_PORTS`
- At least one node must be specified
- All nodes should be accessible from the server

**Example:**
```bash
NODE_HOSTS=["192.168.1.10","192.168.1.11","192.168.1.12"]
```

---

#### NODE_GRPC_PORTS

```bash
NODE_GRPC_PORTS=[40451,40451,40451]
```

**Description:** Array of gRPC port numbers for each validator node.

**Format:** JSON array of integers

**Requirements:**
- Must have same length as `NODE_HOSTS` and `NODE_HTTP_PORTS`
- Each port corresponds to the node at the same index in `NODE_HOSTS`
- Default validator gRPC port: 40451

**Example:**
```bash
NODE_GRPC_PORTS=[40451,40452,40453]
```

---

#### NODE_HTTP_PORTS

```bash
NODE_HTTP_PORTS=[40453,40453,40453]
```

**Description:** Array of HTTP port numbers for each validator node.

**Format:** JSON array of integers

**Requirements:**
- Must have same length as `NODE_HOSTS` and `NODE_GRPC_PORTS`
- Each port corresponds to the node at the same index in `NODE_HOSTS`
- Default validator HTTP port: 40453

**Example:**
```bash
NODE_HTTP_PORTS=[40453,40454,40455]
```

---

#### READONLY_HOST

```bash
READONLY_HOST=observer.asi.io
```

**Description:** Hostname or IP address of the read-only observer node used for balance queries and deploy status checks.

**Format:** String (hostname or IP)

**Purpose:**
- Balance queries
- Transaction status checks
- Does not perform writes
- Reduces load on validator nodes

**Example:**
```bash
READONLY_HOST=192.168.1.100
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

**Unit:** Smallest token unit (1 ASI = 10^8 or 10^9 units depending on token decimals)

**Default:** 10000

**Validation:** Must be greater than 0

**Example:**
```bash
FAUCET_AMOUNT=50000  # Send 50,000 smallest units per request
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

**Calculation:** Balance is converted to smallest unit before comparison: `max_balance_allowed = FAUCET_MAX_BALANCE * 10^8`

**Example:**
```bash
FAUCET_MAX_BALANCE=10000  # Addresses with 10,000+ ASI are ineligible
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

## Configuration File

### .env File Structure

Create a `.env` file in the `server/` directory:

```bash
# Faucet Configuration
FAUCET_AMOUNT=10000
FAUCET_MAX_BALANCE=20000
PRIVATE_KEY=<your_private_key>

# Validator Nodes
NODE_HOSTS=["validator1.asi.io","validator2.asi.io","validator3.asi.io"]
NODE_GRPC_PORTS=[40451,40451,40451]
NODE_HTTP_PORTS=[40453,40453,40453]

# Read-Only Observer Node
READONLY_HOST=observer.asi.io
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

### Production Configuration

```bash
# Use production-grade nodes
NODE_HOSTS=["prod-validator1.asi.io","prod-validator2.asi.io"]
READONLY_HOST=prod-observer.asi.io

# Strict logging
RUST_LOG=asi_faucet=info,tower_http=warn

# Reasonable limits
FAUCET_AMOUNT=5000
FAUCET_MAX_BALANCE=10000
```

### Development Configuration

```bash
# Use testnet nodes
NODE_HOSTS=["testnet-validator1.asi.io"]
READONLY_HOST=testnet-observer.asi.io

# Verbose logging
RUST_LOG=asi_faucet=debug,tower_http=debug

# Generous limits for testing
FAUCET_AMOUNT=100000
FAUCET_MAX_BALANCE=50000
```

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

### Kubernetes Deployment

Use ConfigMaps for non-sensitive data and Secrets for sensitive data:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: faucet-config
data:
  FAUCET_AMOUNT: "10000"
  FAUCET_MAX_BALANCE: "20000"
  SERVER_PORT: "40470"
```

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: faucet-secrets
type: Opaque
stringData:
  PRIVATE_KEY: "<your_private_key>"
```

---

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
# Test gRPC port
nc -zv validator1.asi.io 40451

# Test HTTP port  
curl http://validator1.asi.io:40453
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
