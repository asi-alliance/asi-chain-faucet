# Configuration Reference

Complete configuration guide for ASI Chain Faucet backend server.

---

## Overview

### Exact amounts and native precision

`FAUCET_TOKEN_DECIMALS` is an integer in 0..18, default 8. The selected node
must report the same precision before the faucet signs a transfer. Its own
HTTP status supplies the minimum phlo price used for that submission.
`FAUCET_AMOUNT` remains an unsigned atomic-unit decimal string, now bounded by
uint256 instead of u64. For example, ten tokens plus one atomic unit on an
18-decimal test network is `10000000000000000001`.

`FAUCET_MAX_BALANCE` retains the server's existing **whole-token** interpretation.
It accepts an exact decimal token string and scales it once using configured
precision. Eligibility is strictly `balance < ceiling`; equality is ineligible.
The threshold applies to the balance before distribution, not the resulting
post-transfer balance. It is not a cumulative claim or rate limit.
The historical defaults remain unchanged. Review them explicitly when changing
network precision. Invalid or overflowing monetary configuration fails startup.

`GET /config` exposes only native precision, atomic distribution amount, and
atomic eligibility ceiling as exact JSON values. The frontend reads these
server settings together with the balance. Its legacy `VITE_TOKEN_DECIMALS`
and `VITE_FAUCET_BALANCE_LIMIT` settings are ignored; `/config` is authoritative.
Unavailable, malformed, negative, or inexact balance responses do not enable claims.
Balance reads retain the system-vault exploratory query and the configured
`EXPLORATORY_RETRY_BUDGET_SEC` for observer capacity refusals.

The backend uses [asi-alliance/asi-chain-client, branch `rust-dev`](https://github.com/asi-alliance/asi-chain-client/tree/rust-dev)
as a Cargo git dependency pinned to `5d1c9b4d12e33ec8b64a74a28df196ef03478e73`.
Cargo fetches it automatically; `Cargo.lock` records the exact Git source.
Node protocol dependencies remain at `0ccc315d84b32d668e8db49cd56ba3c576693e15`.
Phlo prices must fit positive `i64` and are encoded as uint256 protobuf values.
The validator and observer must support the uint256 monetary wire protocol.
The ASI system-vault transfer ABI and exact balance query remain in use.
HTTP integer and Phlo price responses accept string and legacy numeric encodings.
Balances and transfer amounts remain exact uint256 values. A minimum Phlo price
outside the client i64 range fails before signing.
Build with the committed `nightly-2026-02-09` toolchain and `Cargo.lock`.

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
FAUCET_AMOUNT=1000000000000
```

**Description:** Amount of ASI tokens in smallest token unit to send per transfer request.

**Format:** Integer (positive, non-zero)

**Unit:** Atomic units. One ASI is `10^FAUCET_TOKEN_DECIMALS` atomic units (default 8 decimals).

**Default:** 1000000000000

**Validation:** Must be greater than 0

**Example:**
```bash
FAUCET_AMOUNT=1000000000000  # Send 10,000 ASI per request
```

---

#### FAUCET_MAX_BALANCE

```bash
FAUCET_MAX_BALANCE=2000000000000
```

**Description:** Pre-claim eligibility ceiling in whole ASI tokens. A balance equal to the ceiling is ineligible.

**Format:** Positive decimal token string, exactly representable at configured precision and within uint256 atomic units.

**Unit:** Whole ASI tokens. The server's historical whole-token interpretation is retained.

**Default:** 2000000000000

**Purpose:** Checks the observed balance before a claim. This is not rate limiting or pending-claim reconciliation; see issue #44.

**Example:**
```bash
FAUCET_MAX_BALANCE=20000  # Addresses with 20,000+ ASI are ineligible; explicitly overrides the historical default
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

#### ALERTS_ENABLED

```bash
ALERTS_ENABLED=false
```

**Description:** Master switch for critical-error alerts posted to a Mattermost channel.

**Default:** `false`

**Notes:** When `false`, the service behaves exactly as before and no outgoing webhook requests are made. When `true`, `MATTERMOST_WEBHOOK_URL` becomes required and the server refuses to start without it.

---

#### MATTERMOST_WEBHOOK_URL

```bash
MATTERMOST_WEBHOOK_URL=https://mattermost.example.com/hooks/xxxxxxxxxxxxxxxxx
```

**Description:** Mattermost incoming webhook URL that receives alert messages.

**Default:** none

**Required:** only when `ALERTS_ENABLED=true`

**Notes:** Treated as a secret — it is masked as `***` in debug logs and never included in alert message bodies.

---

#### MATTERMOST_CHANNEL

```bash
MATTERMOST_CHANNEL=asi-faucet-alerts
```

**Description:** Overrides the channel configured on the webhook itself.

**Default:** unset (the webhook's own default channel is used)

---

#### MATTERMOST_USERNAME

```bash
MATTERMOST_USERNAME=asi-faucet
```

**Description:** Display name the alert is posted under.

**Default:** `asi-faucet`

---

#### ALERT_THROTTLE_SEC

```bash
ALERT_THROTTLE_SEC=300
```

**Description:** Deduplication window, in seconds, applied per event type. Repeats of the same event type inside the window are suppressed; the next message sent after the window reports how many repeats were suppressed.

**Default:** `300`

---

#### ALERT_TIMEOUT_SEC

```bash
ALERT_TIMEOUT_SEC=5
```

**Description:** Timeout, in seconds, for a single webhook delivery attempt. Delivery runs off the request path, so this never delays an HTTP response.

**Default:** `5`

---

#### ALERT_ENVIRONMENT

```bash
ALERT_ENVIRONMENT=staging
```

**Description:** Environment label shown in the alert header, so messages from different deployments are distinguishable.

**Default:** `unknown`

---

## Critical Error Alerts

When enabled, the service posts a message to a Mattermost channel for two event types only:

| Event | Raised when |
|-------|-------------|
| `No reachable nodes` | A transfer request is handled and none of the configured validator nodes reports readiness |
| `Transfer deploy failed` | The transfer deploy to a validator node returns an error |

Everything else stays in the logs only — balance query failures, deploy status failures, startup and configuration errors, and user validation errors (invalid address, invalid deploy id, balance above the faucet threshold).

Each message carries the environment label, service name, severity, event type, error text, event context (recipient address or the polled node list), the `x-request-id` of the originating request, and a UTC timestamp.


---

## Token Decimals and Frontend Integration

The backend validates `FAUCET_TOKEN_DECIMALS` in 0..18 and checks the selected
node's reported precision before signing. Monetary configuration errors fail startup.
The frontend reads `/config` for precision and atomic eligibility limits, so
claim decisions use the same units as the backend. Deploy this frontend with
the matching backend; an old backend without `/config` leaves claims unavailable.

For an isolated native18 configuration:

```bash
FAUCET_TOKEN_DECIMALS=18
FAUCET_AMOUNT=10000000000000000001
FAUCET_MAX_BALANCE=100
```

This distributes `10.000000000000000001` ASI to an address whose observed
pre-claim balance is below 100 ASI. Transfer amounts and ceilings are bounded
by uint256. Fee prices also support uint256 and use exactly 32 big-endian bytes
at protobuf field 15 in signed deploys; the former field 7 is no longer used.

---

## Configuration File

### .env File Structure

Create a `.env` file in the `server/` directory:

```bash
# Faucet Configuration
FAUCET_AMOUNT=1000000000000
FAUCET_MAX_BALANCE=2000000000000
PRIVATE_KEY=<your_private_key>

# Validator Nodes
NODE_HOSTS=["http://node1.asi.io","http://node2.asi.io","http://node3.asi.io"]
NODE_GRPC_PORTS=[40412,40422,40432]
NODE_HTTP_PORTS=[40413,40423,40433]

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

# Alerts (Mattermost incoming webhook)
ALERTS_ENABLED=false
MATTERMOST_WEBHOOK_URL=
MATTERMOST_CHANNEL=
MATTERMOST_USERNAME=asi-faucet
ALERT_THROTTLE_SEC=300
ALERT_TIMEOUT_SEC=5
ALERT_ENVIRONMENT=unknown
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

4. **Conditionally Required Variables:**
   - `MATTERMOST_WEBHOOK_URL` must be set when `ALERTS_ENABLED=true`

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

```
Error: MATTERMOST_WEBHOOK_URL environment variable is required when ALERTS_ENABLED=true
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
