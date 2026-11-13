# Configuration Reference

Complete configuration guide for ASI Chain Faucet backend server.

---

## Overview

### Exact amounts and native precision

`FAUCET_TOKEN_DECIMALS` is an integer in 0..18, default 18. The selected node
must report the same precision before the faucet signs a transfer. Its own
HTTP status supplies the minimum phlo price used for that submission.
`FAUCET_AMOUNT` and `FAUCET_MAX_BALANCE` both use ASI token decimal strings:
`10000` means 10,000 ASI, regardless of configured precision. Fractional ASI
values are accepted only when exactly representable at that precision. Both
values are scaled once at startup into exact uint256 atomic units. Eligibility
is strictly `balance < ceiling`; equality is ineligible.
The threshold applies to the balance before distribution, not the resulting
post-transfer balance. It is not a cumulative claim or rate limit.
The default eligibility ceiling is 20,000 whole ASI tokens. Review monetary
settings explicitly when changing network precision. Invalid or overflowing monetary configuration fails startup.
Effective amounts and ceilings are logged at
`info` level in both ASI and atomic units, with the configured precision.

`GET /config` exposes native precision, atomic distribution amount, atomic
eligibility ceiling and the eligibility policy. The frontend reads these
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

The backend reads environment variables and a `.env` file at startup. Monetary
settings, required secrets and deploy parameters are validated before serving.
Observer and validator connectivity is checked by actual requests.

---

## Environment Variables

### Startup and Transfer Settings

`PRIVATE_KEY` is required at startup. The validator arrays below must be set
correctly for transfers to succeed.

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

**Format:** Bracketed, comma-separated hostnames or IP addresses without a URL scheme or port

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

#### OBSERVER_HOST

```bash
OBSERVER_HOST=observer.example.com
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
OBSERVER_HOST=localhost
# or with custom host:
OBSERVER_HOST=observer.example.com
# or with IP:
OBSERVER_HOST=192.168.1.20
```

---

### Optional Variables

These variables have default values and can be omitted:

#### FAUCET_AMOUNT

```bash
FAUCET_AMOUNT=10000
```

**Description:** Amount of ASI tokens to send per transfer request.

**Format:** Positive decimal ASI token string, exactly representable at configured precision.

**Unit:** ASI tokens, the same unit as `FAUCET_MAX_BALANCE`. Both are converted using `FAUCET_TOKEN_DECIMALS` (default 18).

**Default:** 10000

**Validation:** Must be greater than 0 and fit uint256 atomic units. The
distribution amount is independent of the pre-claim eligibility ceiling.

**Example:**
```bash
FAUCET_AMOUNT=10000  # Send 10,000 ASI per request
```

---

#### FAUCET_MAX_BALANCE

```bash
FAUCET_MAX_BALANCE=20000
```

**Description:** Pre-claim eligibility ceiling in whole ASI tokens. A balance equal to the ceiling is ineligible.

**Format:** Positive decimal token string, exactly representable at configured precision and within uint256 atomic units.

**Unit:** Whole ASI tokens. The server's historical whole-token interpretation is retained.

**Default:** 20000

**Purpose:** Checks the observed balance before a claim. Durable pending-claim reconciliation is separate (see the claim reservations section below), as is rate limiting.

**Example:**
```bash
FAUCET_MAX_BALANCE=20000  # Addresses with 20,000+ ASI are ineligible
```

---

#### OBSERVER_GRPC_PORT

```bash
OBSERVER_GRPC_PORT=40452
```

**Description:** gRPC port of the read-only observer node.

**Format:** Integer (1-65535)

**Default:** 40452

**Used For:**
- Balance queries via gRPC

**Example:**
```bash
OBSERVER_GRPC_PORT=40452
```

---

#### OBSERVER_HTTP_PORT

```bash
OBSERVER_HTTP_PORT=40453
```

**Description:** HTTP port of the read-only observer node.

**Format:** Integer (1-65535)

**Default:** 40453

**Used For:**
- Deploy status queries via HTTP

**Example:**
```bash
OBSERVER_HTTP_PORT=40453
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

**Description:** Controls the number of status attempts within one `/deploy/{id}` request.

**Format:** Integer (positive)

**Default:** 6

**Purpose:** Sets the polling budget together with `DEPLOY_CHECK_INTERVAL_SEC`.
It is not a wall-clock deadline: node requests add latency, and the `/deploy`
route has an independent seven-second timeout.

**Calculation:** Maximum attempts = `max(1, DEPLOY_MAX_WAIT_SEC / DEPLOY_CHECK_INTERVAL_SEC)` using integer division.

**Example:**
```bash
DEPLOY_MAX_WAIT_SEC=10  # Configure five attempts at the default two-second interval
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

**Calculation:** Number of attempts = `max(1, DEPLOY_MAX_WAIT_SEC / DEPLOY_CHECK_INTERVAL_SEC)` using integer division.

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

**Default:** `asi_faucet=info,tower_http=debug,axum=debug`

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

When enabled, the service posts these events to a Mattermost channel:

| Event | Raised when |
|-------|-------------|
| `No reachable nodes` | A transfer request is handled and none of the configured validator nodes reports readiness |
| `Transfer deploy failed` | The transfer deploy to a validator node returns an error |
| `Deploy failed` | Deploy polling reports execution failure, expiry, or a rejected vault transfer; finalized-claim reconciliation also reports execution or vault-transfer errors from available deploy detail |

Other failures stay in the logs, including balance query failures, deploy status failures, startup and configuration errors, and user validation errors.

Each message carries the environment label, service name, event type, error
text and event context. Signed deploy bytes are not included.


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
FAUCET_AMOUNT=10.000000000000000001
FAUCET_MAX_BALANCE=100
```

This distributes `10.000000000000000001` ASI to an address whose observed
pre-claim balance is below 100 ASI. Transfer amounts and ceilings are bounded
by uint256. Fee prices must fit positive `i64` in the pinned client and use
exactly 32 big-endian bytes at protobuf field 15 in signed deploys.

---

## Configuration File

### .env File Structure

Create a `.env` file in the `server/` directory:

```bash
# Faucet Configuration
FAUCET_TOKEN_DECIMALS=18
FAUCET_AMOUNT=10000
FAUCET_MAX_BALANCE=20000
FAUCET_CLAIMS_DIR=.faucet-claims
FAUCET_NETWORK_ID=default
PRIVATE_KEY=<your_private_key>

# Validator Nodes
NODE_HOSTS=["node1.asi.io","node2.asi.io","node3.asi.io"]
NODE_GRPC_PORTS=[40412,40422,40432]
NODE_HTTP_PORTS=[40413,40423,40433]

# Read-Only Observer Node
OBSERVER_HOST=observer.asi.io
OBSERVER_GRPC_PORT=40452
OBSERVER_HTTP_PORT=40453

# Server Configuration
SERVER_HOST=0.0.0.0
SERVER_PORT=40470

# Deploy Status Checking
DEPLOY_MAX_WAIT_SEC=6
DEPLOY_CHECK_INTERVAL_SEC=2
DEPLOY_PHLO_LIMIT=500000
EXPLORATORY_RETRY_BUDGET_SEC=20

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

Startup validation covers the following settings:

### Validation Rules

1. **Required secret:** `PRIVATE_KEY` must be non-empty. The signing key's
   hexadecimal format is checked when preparing a transfer.
2. **Monetary values:** `FAUCET_TOKEN_DECIMALS` must be an integer in 0..18.
   `FAUCET_AMOUNT` and `FAUCET_MAX_BALANCE` must be positive ASI decimal
   strings, exactly representable at that precision and within uint256 atomic
   units. Payout size is independent of the pre-claim eligibility ceiling.
3. **Deploy parameters:** `DEPLOY_CHECK_INTERVAL_SEC` and `DEPLOY_PHLO_LIMIT`
   must be positive.
4. **Conditional secret:** `MATTERMOST_WEBHOOK_URL` must be non-empty when
   `ALERTS_ENABLED=true`.

The validator host and port arrays must be non-empty, parseable and have equal
lengths for transfers to work. The current loader falls back to an empty node
list when these arrays are missing or malformed, so this does not stop startup;
a claim that reaches validator selection then returns 503. Most optional
integer settings fall back to their documented defaults when parsing fails.
`FAUCET_NETWORK_ID` and journal storage are checked when opening the claim store.
No observer endpoint probe runs at startup or before signing; HTTP 404 is
reported when a real deploy status is queried.

### Validation Errors

If validation fails, the server will exit with a clear error message:

```
Error: PRIVATE_KEY environment variable is required
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

**No reachable validators / claim returns 503**
- Count elements in each array
- Ensure all arrays have identical lengths
- Check JSON array format: `["item1","item2"]`

**Malformed NODE_HOSTS configuration**
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
OBSERVER_HOST=localhost
EOF

# Starts without a node connectivity probe; node operations fail when requested
cargo run
```

---

For API documentation, see [API.md](API.md).  
For development information, see [DEVELOPMENT.md](DEVELOPMENT.md).

## Durable claim reservations (issue #44)

The ceiling is a strict **pre-claim eligibility threshold**, not a maximum
post-distribution balance. `/config` reports `eligibility_policy` as
`strict_pre_claim_threshold`; the UI displays the threshold. All monetary
values and journal amounts remain exact uint256 atomic-unit decimal strings.
Rate limiting is a separate infrastructure policy.

`FAUCET_CLAIMS_DIR` defaults to `.faucet-claims`. It must be a persistent local
filesystem with file locking, atomic rename and fsync support. Compose mounts
`/data/claims` on the `faucet-claims` named volume. Preserve this directory when
replacing a container or migrating the service. Do not run replicas with
independent journals; processes on the same host must share the same directory.

`FAUCET_NETWORK_ID` defaults to `default` and namespaces reservations. Set a
stable unique ID for each chain, and keep it unchanged when rotating observer
or validator endpoints, keys or monetary settings. Changing it bypasses the
old namespace and must only be done when switching chains.

Admission locks each network/recipient before reading balance. The exact signed
deploy, its ID and distribution amount are atomically journaled and fsynced
before submission. Concurrent requests return 409 while that lock is held.
Pending retries return the existing ID and may resubmit the **same signed
bytes**. An uncertain submission still returns that ID for status polling;
it never opens another claim. Restart and cancellation retain the journal. The prepared validator endpoint is
stored with the signed bytes, so retries use the same submission node.

Reservations are reconciled on the next claim. Only node-reported `Failed`,
`Expired` or `Finalized` permit a new eligibility check; elapsed wall time,
missing status, observer failures and malformed journals never release a claim.
For `Finalized`, reconciliation uses the observer HTTP balance API to verify
the inclusion block and then read the current finalized balance at a height
at least as high as inclusion. This permits later claims after spending while
preventing stale pre-distribution balances from reopening eligibility.
Reconciliation also checks available deploy detail and raises the existing
`DeployFailed` alert for an execution or vault-transfer error. A rejected
transfer remains canonically `Finalized`, so the inclusion-block balance guard
still applies. This check runs on a claim retry and needs no background worker.
The observer must support both HTTP endpoints:

- `/api/balance/{address}` for finalized balance reconciliation.
- `/api/deploy-finalization-status/{id}` for deploy finalization status.

An unknown deploy returns HTTP 200 `Pending`; HTTP 404 means the endpoint
is unavailable or unsupported. A 404 is reported as an error when querying
deploy status and fails existing-claim reconciliation with HTTP 503, keeping
the reservation rather than masquerading as `Pending`.
Inability to verify finalized balances also keeps the reservation.
Initial balance reads retain the exploratory API and its
configured capacity retry budget. New admission returns 400 if ineligible,
or 503 if reconciliation/storage is unavailable. Status is queried when a
client polls a deploy or retries a claim. There is no background polling,
and elapsed time never clears a reservation.


When upgrading an existing installation, explicitly set `FAUCET_AMOUNT=10000`
and `FAUCET_MAX_BALANCE=20000` in its `.env` or deployment environment.
`FAUCET_AMOUNT` changed from atomic units to ASI tokens: the old documented
`1000000000000` now requests one trillion ASI. The old ceiling example
`2000000000000` also means two trillion whole ASI; `FAUCET_MAX_BALANCE`
continues to use whole tokens. Do not copy either old numeric value unchanged.

The `FAUCET_TOKEN_DECIMALS` default changed from 8 to 18. Explicitly set it to
the actual node runtime precision, including deployments that previously set
only `FAUCET_AMOUNT`; do not override precision only in the frontend. Review
all three monetary settings together and check their effective ASI and atomic
values in the startup info logs. Numeric validation cannot infer whether a
valid token amount was intended as atomic units. Existing journals retain their original atomic
amount and signed bytes after a configuration correction; inspect them using
the procedure below instead of assuming a restart changes the pending deploy.

### Inspecting and recovering a stuck reservation

A pending reservation keeps the recipient reserved. HTTP 200 from
`/transfer` with the same deploy ID acknowledges that reservation; it does not
prove submission or a successful transfer. Time alone is never proof that a
deploy cannot execute.

1. Identify the recipient and deploy ID from the claim response or error log.
   Files live at `FAUCET_CLAIMS_DIR/<hex-encoded FAUCET_NETWORK_ID>/<address>.json`.
   Inspect `deploy_id` and `amount` (atomic units). `signed` holds
   the serialized prepared transfer, including the original validator endpoint
   and exact signed deploy bytes. Keep journal backups access-restricted and
   avoid pasting `signed` into logs or tickets.
2. Query the configured observer's
   `/api/deploy-finalization-status/<deploy_id>` and `/api/deploy/<deploy_id>`.
   Check observer synchronization, validator availability, deploy expiry and
   any execution or vault-transfer errors. A 404 on finalization status means
   the observer is unsupported; HTTP 200 `Pending` also covers unknown deploys
   and does not prove a transfer was never sent.
3. Restore the validator or observer as needed. Retry `/transfer` for the same
   recipient to resubmit the same signed bytes. For `Failed` or `Expired`, the
   next claim automatically checks eligibility again. For `Finalized`, the
   next claim verifies the inclusion block and current finalized balance at
   or beyond that height. Normal recovery requires no journal deletion.
4. Only the designated faucet/chain operator may remove a recipient `.json`
   as exceptional manual recovery, after verifying the chain outcome and that
   the old deploy cannot execute again. Stop **all** faucet instances sharing
   the volume first and back up the network directory, preserving timestamps.
   Confirm terminal failure/expiry, or confirmed finalization and an observer
   caught up with the resulting balance. If status remains pending or unknown,
   retain the reservation unless the chain operator has independent proof
   that the signed deploy can no longer execute. Record that evidence and the
   recovery decision, then remove only the affected `.json`; preserve `.lock`
   files, other recipients, the volume and the network ID.
5. Removing a `.json` discards the service's memory of that claim. After restart,
   the next eligible request can sign a new deploy. Deletion cannot cancel a
   deploy already submitted: if it later executes, the recipient may receive
   both transfers. If the original outcome or observer freshness is uncertain,
   leave the journal in place and continue investigating.


### Current ASItestnet deployment (18 decimals)

```dotenv
FAUCET_TOKEN_DECIMALS=18
FAUCET_AMOUNT=10000
FAUCET_MAX_BALANCE=20000
FAUCET_NETWORK_ID=ASItestnet-native18-2026-11-11
```

This sends 10,000 ASI per claim. The 20,000 ASI eligibility threshold becomes
`20000000000000000000000` atomic units in `/config` and server comparisons.
`FAUCET_MAX_BALANCE` remains whole tokens; do not put its atomic value in `.env`.
This namespace identifies the recreated 18-decimal chain. Old-chain journals
are retained under their original namespace and are not replayed into this one.
Use a new namespace only when recreating/switching chains, never for an ordinary
precision setting correction on the same chain.
The frontend reads precision from `/config`, displays whole ASI or up to four
fractional digits, and marks truncated values with `≈` (dust with `<0.0001`).
Display shortening never changes exact integer eligibility comparisons.
