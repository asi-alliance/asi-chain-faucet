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

All errors follow a consistent format:

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
| to_address | string | Yes | Valid ASI address (must start with "1111", 50-54 characters, alphanumeric) |

**Success Response (200 OK):**

```json
{
  "deploy_id": "d1f2e3b4a5c6789012345678901234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef12"
}
```

**Response Fields:**

| Field | Type | Description |
|-------|------|-------------|
| deploy_id | string | Unique identifier for the transfer transaction (100-160 characters) |

**Error Responses:**

Invalid address format (400 Bad Request):
```json
{
  "error": "Validation Error",
  "details": "Address must start with 1111",
  "timestamp": "2025-10-29T12:34:56.789Z"
}
```

Balance exceeds limit (400 Bad Request):
```json
{
  "error": "Validation Error",
  "details": "Address balance exceeds faucet eligibility threshold",
  "timestamp": "2025-10-29T12:34:56.789Z"
}
```

Transfer failed (400 Bad Request):
```json
{
  "error": "FAUCET: Transfer failed",
  "details": "Insufficient funds in faucet wallet",
  "timestamp": "2025-10-29T12:34:56.789Z"
}
```

**Validation Rules:**

1. **Address Format:**
   - Must start with "1111"
   - Length between 50-54 characters
   - Only alphanumeric characters allowed

2. **Balance Check:**
   - Recipient balance must be below `FAUCET_MAX_BALANCE` (default: 20,000 ASI)
   - Balance checked before transfer is initiated

3. **Amount:**
   - Transfer amount is configured via `FAUCET_AMOUNT` environment variable
   - Default: 10,000 units (smallest unit)

**Processing Flow:**

1. Validate address format
2. Query recipient balance from read-only observer node
3. Check balance against faucet limit
4. Select random validator node with availability check:
   - Nodes are shuffled randomly
   - Each node is checked for availability via HTTP `/status` endpoint with 2 second timeout
   - First available node is selected
   - Request fails if no nodes are reachable
5. Initiate transfer using private key via node CLI
6. Return deploy ID to client

**Note on Transfer Timeout:** The transfer operation currently uses hardcoded timeout values (`max_wait: 60 seconds`, `check_interval: 5 seconds`) that differ from the configurable `DEPLOY_MAX_WAIT_SEC` and `DEPLOY_CHECK_INTERVAL_SEC` used in deploy status queries.

**Response Time:**
- Typical: 1-3 seconds
- Maximum: 7 seconds (enforced by timeout middleware)

**Status Codes:**
- `200 OK` - Transfer successfully initiated
- `400 Bad Request` - Invalid address or balance exceeds limit
- `500 Internal Server Error` - Server error during transfer

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
| balance | string | Balance in smallest unit (motes). In this implementation, 1 ASI = 10^8 motes (8 decimal places) |

**Error Responses:**

Invalid address format (400 Bad Request):
```json
{
  "error": "Validation Error",
  "details": "Address must start with 1111",
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
- Uses `wallet_balance_command` from node_cli
- Connects to read-only observer node via gRPC (port 40452)
- Returns raw balance string without conversion
- Balance is returned in the smallest unit (motes)
- Backend uses 10^8 as the conversion factor for balance calculations
- Frontend should use `VITE_TOKEN_DECIMALS=8` to correctly display balances

**Response Time:**
- Typical: 100-200ms
- Maximum: 7 seconds (enforced by timeout middleware)

**Status Codes:**
- `200 OK` - Balance retrieved successfully
- `400 Bad Request` - Invalid address format or error querying blockchain

---

### GET /deploy/:deploy_id

Retrieves the status and information about a deploy transaction.

**Request:**

```http
GET /deploy/d1f2e3b4a5c6789012345678901234567890abcdef1234567890abcdef12 HTTP/1.1
```

**Path Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| deploy_id | string | Yes | Deploy ID from /transfer endpoint (100-160 alphanumeric characters) |

**Success Response (200 OK):**

**Status: Finalized**
```json
{
  "status": "Finalized",
  "msg": "Transfer completed successfully",
  "block_hash": "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef12"
}
```

**Status: Deploying**
```json
{
  "status": "Deploying"
}
```

**Status: Finalizing**
```json
{
  "status": "Finalizing"
}
```

**Status: DeployError**
```json
{
  "status": "DeployError",
  "msg": "Insufficient funds"
}
```

**Status: FinalizationError**
```json
{
  "status": "FinalizationError",
  "msg": "Block finalization failed"
}
```

**Response Fields:**

| Field | Type | Required | Description |
|-------|------|----------|-----------|
| status | string | Yes | Deploy status (see below) |
| msg | string | No | Human-readable status message |
| block_hash | string | No | Block hash if deploy is included in a block |

**Deploy Status Values:**

| Status | Description |
|--------|-------------|
| Deploying | Transaction submitted, waiting for inclusion in block |
| Finalizing | Transaction included in block, waiting for finalization |
| Finalized | Transaction successfully finalized in block |
| DeployError | Error during transaction deployment |
| FinalizationError | Error during block finalization |

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
- Uses `check_deploy_status` from node_cli
- Connects to read-only observer node via HTTP (port 40453)
- Retries based on `DEPLOY_MAX_WAIT_SEC` and `DEPLOY_CHECK_INTERVAL_SEC`
- Maximum wait time: 6 seconds (configurable)
- Check interval: 2 seconds (configurable)

**Response Time:**
- Typical: 200-500ms
- Maximum: 7 seconds (enforced by timeout middleware)

**Status Codes:**
- `200 OK` - Deploy status retrieved successfully (even if status is error)
- `400 Bad Request` - Invalid deploy ID format
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
curl http://localhost:40470/deploy/d1f2e3b4a5c6789012345678901234567890abcdef12
```



## Error Codes Summary

| HTTP Status | Error Type | Description |
|-------------|------------|-------------|
| 400 | Validation Error | Invalid input format or business rule violation |
| 408 | Request Timeout | Request exceeded 7 second timeout |
| 413 | Payload Too Large | Request body exceeds 1MB limit |
| 500 | Internal Server Error | Server-side error during processing |

---

For configuration details, see [CONFIGURATION.md](CONFIGURATION.md).  
For development information, see [DEVELOPMENT.md](DEVELOPMENT.md).
