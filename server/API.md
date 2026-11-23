# API Reference

Complete API documentation for ASI Chain Faucet backend server.

---

## Base URL

```
http://localhost:40470
```

For production deployments, replace with your actual server URL.

---

## Common Response Format

### Success Response

All successful responses return JSON with appropriate status code and data.

### Error Response

Application-level validation and node errors use this format. Framework
rejections, including malformed request bodies and route timeouts, can use a
different response body.

```json
{
  "error": "Brief error description",
  "details": "Additional error context (optional)",
  "timestamp": "2025-10-24T12:34:56.789Z"
}
```

---

## Endpoints

### POST /transfer

Transfers test ASI tokens to a specified address after validating the recipient's balance.

**Request:**

```http
POST /transfer HTTP/1.1
Content-Type: application/json

{
  "to_address": "11114GuXVLzHJqUqDUJGLJJsn8c1ASIhztKZtG1KN1jV48XPBUdVzKBD3R"
}
```

**Request Body:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| to_address | string | Yes | Vault address starting with "1111", at least 40 characters, valid base58 |

**Success Response (200 OK):**

```json
{
  "deploy_id": "d1f2e3b4a5c6789012345678901234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef12"
}
```

**Response Fields:**

| Field | Type | Description |
|-------|------|-------------|
| deploy_id | string | Signed deploy ID (100-160 characters); pending retries return the existing ID |

**Error Responses:**

Every error body has the shape below. `error` carries a machine code from the
table in "Submission errors"; other failures use a human-readable message.
`details` may be `null`, and `deploy_id` is omitted when it does not apply.

```json
{
  "error": "deploy_pool_busy",
  "details": "deploy_pool_busy: the deploy pool would exceed its configured limit. Retry later or submit to another validator.",
  "timestamp": "2025-10-29T12:34:56.789Z",
  "deploy_id": "3045022100..."
}
```

Invalid address format (400 Bad Request):
```json
{
  "error": "Validation Error",
  "details": "invalid vault address: must start with '1111'",
  "timestamp": "2025-10-29T12:34:56.789Z"
}
```

Balance at or above the eligibility ceiling (400 Bad Request):
```json
{
  "error": "Address balance must be strictly below the pre-claim eligibility threshold.",
  "details": "Address balance must be strictly below the pre-claim eligibility threshold.",
  "timestamp": "2025-10-29T12:34:56.789Z"
}
```

Claim reconciliation, preparation or journal storage unavailable (503 Service Unavailable):
```json
{
  "error": "Claim unavailable. Any existing reservation is retained; retry later.",
  "details": "Claim unavailable. Any existing reservation is retained; retry later.",
  "timestamp": "2025-10-29T12:34:56.789Z"
}
```

Submission errors carry the original `deploy_id` and keep its reservation.
A retry for the same recipient may resend only the journaled bytes. When the
node supplies a queue retry hint, the response also sets `Retry-After` in
seconds.

| HTTP | Error code | Meaning |
|---|---|---|
| 503 | `deploy_pool_busy`, `deploy_admission_busy`, `invalid_deploy_window` | Temporary admission refusal |
| 503 | `deploy_queue_full` (queue full), `node_unavailable`, `deploy_queue_stopped` | Queue capacity or availability refusal |
| 502 | `deploy_queue_full` (deployer share full) | Queue holds too many of the faucet's deploys |
| 502 | `deploy_too_large`, `deploy_expired` | Admission refused for the signed deploy |
| 502 | `deploy_rejected` | Explicit untyped node rejection |
| 503 | `deploy_not_submitted` | No configured validator passed its pre-submission check; nothing was sent |
| 502 | `deploy_submission_unknown` | No validator confirmed acceptance. Check the returned ID before retrying. |

A deploy queue reports a full queue and a full per-deployer share with the same
`deploy_queue_full` code. A full queue is the 503 refusal that can clear; a
share-full refusal is a 502 that keeps the reservation and alerts, because the
queue already holds too many of the faucet's deploys. A `503` refusal can clear
for a later request, while a `502` refusal means the validators that answered
cannot accept these bytes. The faucet never re-signs a journaled deploy.

When several validators refuse, the response reports the most consequential
outcome: an unknown submission outranks a permanent refusal, which outranks a
temporary one. Validator order never changes the code.

**Validation Rules:**

1. **Address Format:**
   - Must start with "1111"
   - At least 40 characters
   - Must decode as base58

2. **Balance Check:**
   - Recipient balance must be strictly below `FAUCET_MAX_BALANCE`, configured in whole tokens (default: 20,000 ASI)
   - Balance checked before transfer is initiated

3. **Amount:**
   - Transfer amount is configured via `FAUCET_AMOUNT` environment variable
   - Default: 10,000 ASI. `FAUCET_AMOUNT` and `FAUCET_MAX_BALANCE` both use ASI token strings; atomic conversion uses `FAUCET_TOKEN_DECIMALS`.

**Processing Flow:**

1. Validate the address and acquire the network/recipient journal lock; an active claim returns 409.
2. Reconcile an existing reservation. `Pending` retries may resubmit the same signed bytes and return the same deploy ID. Missing status or reconciliation errors return 503 and retain the reservation. Finalized claims require an updated finalized balance.
3. Read the recipient balance and require it to be strictly below the eligibility ceiling before preparing a new claim.
4. Select a random validator node with an availability check:
   - Nodes are shuffled randomly
   - Each node is checked for availability via HTTP `/api/status` endpoint with a 5 second timeout
   - First available node is selected
   - Request fails if no nodes are reachable
5. Verify the validator's native precision, select its minimum phlo price, and prepare and sign the transfer through the Rust-client library.
6. Persist the signed deploy, its ID and amount before submitting via gRPC.
7. Return the deploy ID. A submission error still returns the persisted ID because the node may have accepted the deploy; polling is required to determine its outcome.

**Note on Transfer Timeout:** Transfers return after submission without waiting for finalization. Node connections and individual requests have bounded timeouts (10 and 30 seconds respectively); observer capacity retries use `EXPLORATORY_RETRY_BUDGET_SEC`. There is no outer seven-second deadline cancelling an in-flight submission. `DEPLOY_MAX_WAIT_SEC` and `DEPLOY_CHECK_INTERVAL_SEC` apply to deploy status queries.

**Response Time:**
- Typical: 1-3 seconds
- Bound: balance query, validator availability checks, tip preparation, and submission each use their own node-client timeouts.

**Status Codes:**
- `200 OK` - A signed claim is reserved; this does not prove successful submission or transfer
- `400 Bad Request` - Invalid address or balance at or above the eligibility ceiling
- `409 Conflict` - A concurrent request holds this recipient's claim lock
- `503 Service Unavailable` - Balance, reconciliation, preparation or journal storage is unavailable

---

### GET /balance/:address

Retrieves the current balance of a ASI address from the read-only observer node.

**Request:**

```http
GET /balance/11114GuXVLzHJqUqDUJGLJJsn8c1ASIhztKZtG1KN1jV48XPBUdVzKBD3R HTTP/1.1
```

**Path Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| address | string | Yes | Valid ASI address to query |

**Success Response (200 OK):**

```json
{
  "balance": "1500000000000"
}
```

**Response Fields:**

| Field | Type | Description |
|-------|------|-------------|
| balance | string | Exact uint256 balance in atomic units. One ASI = 10^FAUCET_TOKEN_DECIMALS atomic units (default 18 decimals). |

**Error Responses:**

Invalid address format (400 Bad Request):
```json
{
  "error": "Validation Error",
  "details": "invalid vault address: must start with '1111'",
  "timestamp": "2025-10-29T12:34:56.789Z"
}
```

Query failed (400 Bad Request):
```json
{
  "error": "FAUCET: Balance retrieval failed",
  "details": "Failed to query balance from blockchain",
  "timestamp": "2025-10-29T12:34:56.789Z"
}
```

**Implementation Details:**
- Queries the system vault through the node_cli exploratory API
- Connects to read-only observer node via gRPC (port 40452)
- Returns raw balance string without conversion
- Balance is returned in the smallest unit 
- Backend uses `FAUCET_TOKEN_DECIMALS` for balance calculations
- Frontend reads precision and the atomic eligibility ceiling from `GET /config`

**Response Time:**
- Typical: 100-200ms
- Bound: `EXPLORATORY_RETRY_BUDGET_SEC` plus connection and individual query timeouts.

**Status Codes:**
- `200 OK` - Balance retrieved successfully
- `400 Bad Request` - Invalid address format or error querying blockchain

---

### GET /deploy/:deploy_id

Retrieves the status and information about a deploy transaction.

**Request:**

```http
GET /deploy/<deploy_id-returned-by-transfer> HTTP/1.1
```

**Path Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| deploy_id | string | Yes | Deploy ID from /transfer endpoint (100-160 alphanumeric characters) |

**Success Response (200 OK):**

**Status: Finalized**
```json
{
  "status": "Finalized"
}
```

**Status: Deploying**
```json
{
  "status": "Deploying"
}
```

**Status: DeployError**
```json
{
  "status": "DeployError",
  "msg": "Insufficient funds"
}
```


**Response Fields:**

| Field | Type | Required | Description |
|-------|------|----------|-----------|
| status | string | Yes | Deploy status (see below) |
| msg | string | No | Human-readable status message |

**Deploy Status Values:**

| Status | Description |
|--------|-------------|
| Deploying | Observer reports `Pending`, including deploys it has not yet seen |
| Finalized | Observer reports canonical finalization and no execution or vault-transfer error was found in available detail |
| DeployError | Execution failure, expiry, vault transfer rejection or deploy queue rejection |
| Unknown | Observer returned an unrecognized finalization state |

**Error Responses:**

Invalid deploy ID format (400 Bad Request):
```json
{
  "error": "Validation Error",
  "details": "FAUCET: Invalid deploy ID format (must be 100-160 alphanumeric chars)",
  "timestamp": "2025-10-29T12:34:56.789Z"
}
```

Query failed (500 Internal Server Error):
```json
{
  "error": "Internal Server Error",
  "details": "FAUCET: Failed to retrieve deploy info",
  "timestamp": "2025-10-29T12:34:56.789Z"
}
```

**Implementation Details:**
- Queries `/api/deploy-finalization-status/{id}` and enriches the result with `/api/deploy/{id}` detail when available
- Connects to read-only observer node via HTTP (port 40453)
- Retries based on `DEPLOY_MAX_WAIT_SEC` and `DEPLOY_CHECK_INTERVAL_SEC`
- Attempts per request: `max(1, DEPLOY_MAX_WAIT_SEC / DEPLOY_CHECK_INTERVAL_SEC)` using integer division
- Check interval: 2 seconds (configurable)
- HTTP 404 from the finalization endpoint is a query error, not `Pending`; the faucet returns HTTP 500 and logs the observer error
- Polling does not clear journal reservations; those are reconciled on the next `/transfer` request

**Response Time:**
- Typical: 200-500ms
- Maximum: 7 seconds (enforced by timeout middleware)

**Status Codes:**
- `200 OK` - Deploy status retrieved successfully (even if status is error)
- `400 Bad Request` - Invalid deploy ID format
- `408 Request Timeout` - Deploy status request exceeded its seven-second route timeout
- `500 Internal Server Error` - Error querying blockchain

---

## CORS Configuration

The API accepts requests from any origin with the following configuration:

**Allowed Origins:** `*` (any origin)

**Allowed Methods:**
- GET
- POST
- OPTIONS

**Allowed Headers:**
- Content-Type
- Authorization

**Max Age:** 3600 seconds

For production deployments, it's recommended to restrict allowed origins to your frontend domain.

---

## Rate Limiting

Currently, no rate limiting is implemented at the API layer. Rate limiting should be configured at the infrastructure level for production deployments:

- Reverse proxy (nginx, Caddy)
- API gateway
- Load balancer

Recommended limits:
- General requests: 100 requests/minute per IP
- Transfer requests: 10 requests/hour per IP
- Balance queries: 60 requests/minute per IP

---

## Request/Response Examples

### Complete Transfer Flow

**1. Check current balance:**
```bash
curl http://localhost:40470/balance/11114GuXVLzHJqUqDUJGLJJsn8c1ASIhztKZtG1KN1jV48XPBUdVzKBD3R
```

**2. Request tokens:**
```bash
curl -X POST http://localhost:40470/transfer \
  -H "Content-Type: application/json" \
  -d '{"to_address":"11114GuXVLzHJqUqDUJGLJJsn8c1ASIhztKZtG1KN1jV48XPBUdVzKBD3R"}'
```

**3. Check deploy status:**
```bash
curl "http://localhost:40470/deploy/<deploy_id-returned-by-transfer>"
```



## Error Codes Summary

| HTTP Status | Error Type | Description |
|-------------|------------|-------------|
| 400 | Validation Error | Invalid input format or business rule violation |
| 408 | Request Timeout | Deploy status query exceeded its 7 second route timeout |
| 409 | Conflict | Another request holds the recipient's claim lock |
| 413 | Payload Too Large | Request body exceeds 1MB limit |
| 500 | Internal Server Error | Server-side error during processing |
| 503 | Service Unavailable | Claim reconciliation, preparation or journal storage is unavailable |

---

For configuration details, see [CONFIGURATION.md](CONFIGURATION.md).  
For development information, see [DEVELOPMENT.md](DEVELOPMENT.md).
## Monetary configuration

`GET /config` returns public denomination and eligibility settings:

```json
{"eligibility_policy":"strict_pre_claim_threshold","native_token_decimals":18,"faucet_amount":"10000000000000000000000","eligibility_ceiling":"20000000000000000000000"}
```

This example uses the defaults: 10,000 ASI per claim and a 20,000 ASI ceiling
at 18 decimals. Amounts are atomic-unit strings. Eligibility is strictly below
the pre-claim ceiling. The response contains no signing or service credentials.


### Claim reconciliation

`GET /config` also returns `"eligibility_policy":"strict_pre_claim_threshold"`.
The balance before claiming must be strictly below the atomic ceiling; the
resulting balance may exceed it. Only one deploy per network/recipient may be
pending. `POST /transfer` retries resume the existing signed deploy and include
its ID on success or submission failure, including after a restart.

A concurrent active request returns 409 (retry shortly). Reconciliation or
journal failures return 503 and preserve existing reservations. Node-reported
canonical terminal states are reconciled before admitting another distribution.
Finalized claims require an updated finalized balance. A queue-local rejection
leaves the reservation in place and raises an operator alert; it clears only if
another validator finalizes the deploy or the reservation is removed with the
manual procedure. See CONFIGURATION.md for required persistent storage and
network namespace settings.

The backend does not poll reservations in the background or release them on a
timer. Closing the browser or ending a polling session does not cancel the
deploy. See the [manual recovery procedure](CONFIGURATION.md#inspecting-and-recovering-a-stuck-reservation)
for claims that remain pending.
