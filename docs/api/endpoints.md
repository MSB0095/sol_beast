# API Reference - REST Endpoints

SOL BEAST exposes a REST API for frontend communication and external integrations.

## Base URL

```
http://localhost:8080/api
```

## Authentication

Currently, the API does not require authentication. In production, consider adding authentication middleware.

## Endpoints

### Health Check

Check if the backend is running and responsive.

```http
GET /api/health
```

**Response**:
```json
{
  "status": "healthy",
  "mode": "dry",
  "uptime_seconds": 123
}
```

### Bot Status

Get current bot status and statistics.

```http
GET /api/bot/status
```

**Response**:
```json
{
  "running": true,
  "mode": "dry",
  "connected": true,
  "uptime": 123,
  "events_processed": 456,
  "trades_executed": 12,
  "active_holdings": 3
}
```

### Start Bot

Start the trading bot.

```http
POST /api/bot/start
```

**Response**:
```json
{
  "success": true,
  "message": "Bot started successfully"
}
```

### Stop Bot

Stop the trading bot.

```http
POST /api/bot/stop
```

**Response**:
```json
{
  "success": true,
  "message": "Bot stopped successfully"
}
```

### Get Configuration

Retrieve current bot configuration.

```http
GET /api/config
```

**Response**:
```json
{
  "buy_amount_sol": 0.05,
  "max_slippage_bps": 500,
  "take_profit_percentage": 50.0,
  "stop_loss_percentage": 20.0,
  "timeout_seconds": 300,
  "min_token_age_seconds": 60,
  "min_holder_count": 50,
  "min_liquidity_sol": 10.0,
  "helius_sender_enabled": false
}
```

### Update Configuration

Update bot configuration (requires restart to apply).

```http
POST /api/config
Content-Type: application/json
```

**Request Body**:
```json
{
  "buy_amount_sol": 0.1,
  "take_profit_percentage": 75.0
}
```

**Response**:
```json
{
  "success": true,
  "message": "Configuration updated. Restart bot to apply changes."
}
```

### Get Holdings

Get all current holdings.

```http
GET /api/holdings
```

**Response**:
```json
{
  "holdings": [
    {
      "mint": "TokenMintAddress123...",
      "symbol": "TOKEN",
      "amount": 1000000,
      "entry_price": 0.00005,
      "current_price": 0.00006,
      "pnl_percentage": 20.0,
      "pnl_sol": 0.01,
      "entry_time": "2024-01-15T10:30:00Z",
      "age_seconds": 120
    }
  ],
  "total_value_sol": 0.06,
  "total_pnl_sol": 0.01
}
```

### Get Specific Holding

Get details for a specific holding by mint address.

```http
GET /api/holdings/:mint
```

**Parameters**:
- `mint` - Token mint address

**Response**:
```json
{
  "mint": "TokenMintAddress123...",
  "symbol": "TOKEN",
  "amount": 1000000,
  "entry_price": 0.00005,
  "current_price": 0.00006,
  "pnl_percentage": 20.0,
  "pnl_sol": 0.01,
  "entry_time": "2024-01-15T10:30:00Z",
  "age_seconds": 120,
  "take_profit_target": 0.000075,
  "stop_loss_target": 0.00004
}
```

### Get Trade History

Get historical trades.

```http
GET /api/trades?limit=50&offset=0
```

**Query Parameters**:
- `limit` - Number of trades to return (default: 50, max: 200)
- `offset` - Pagination offset (default: 0)

**Response**:
```json
{
  "trades": [
    {
      "id": "trade-123",
      "mint": "TokenMintAddress123...",
      "symbol": "TOKEN",
      "type": "buy",
      "amount_sol": 0.05,
      "amount_tokens": 1000000,
      "price": 0.00005,
      "timestamp": "2024-01-15T10:30:00Z",
      "signature": "TransactionSignature..."
    },
    {
      "id": "trade-124",
      "mint": "TokenMintAddress123...",
      "symbol": "TOKEN",
      "type": "sell",
      "amount_sol": 0.06,
      "amount_tokens": 1000000,
      "price": 0.00006,
      "timestamp": "2024-01-15T10:35:00Z",
      "signature": "TransactionSignature...",
      "pnl_sol": 0.01,
      "pnl_percentage": 20.0,
      "exit_reason": "take_profit"
    }
  ],
  "total": 124,
  "limit": 50,
  "offset": 0
}
```

### Manual Buy

Execute a manual buy transaction (requires --real mode).

```http
POST /api/trade/buy
Content-Type: application/json
```

**Request Body**:
```json
{
  "mint": "TokenMintAddress123...",
  "amount_sol": 0.05,
  "max_slippage_bps": 500
}
```

**Response**:
```json
{
  "success": true,
  "signature": "TransactionSignature...",
  "amount_tokens": 1000000,
  "price": 0.00005
}
```

### Manual Sell

Execute a manual sell transaction (requires --real mode).

```http
POST /api/trade/sell
Content-Type: application/json
```

**Request Body**:
```json
{
  "mint": "TokenMintAddress123...",
  "amount_tokens": 1000000,
  "max_slippage_bps": 500
}
```

**Response**:
```json
{
  "success": true,
  "signature": "TransactionSignature...",
  "amount_sol": 0.06,
  "price": 0.00006,
  "pnl_sol": 0.01,
  "pnl_percentage": 20.0
}
```

## Error Responses

All endpoints return error responses in this format:

```json
{
  "error": "Error message",
  "code": "ERROR_CODE",
  "details": "Additional details if available"
}
```

### Common Error Codes

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `NOT_FOUND` | 404 | Resource not found |
| `BAD_REQUEST` | 400 | Invalid request parameters |
| `UNAUTHORIZED` | 401 | Authentication required |
| `FORBIDDEN` | 403 | Operation not allowed |
| `INTERNAL_ERROR` | 500 | Server error |
| `DRY_MODE` | 403 | Operation requires --real mode |

## Rate Limiting

Currently no rate limiting is implemented. In production, consider adding rate limits.

## CORS

CORS is enabled for `http://localhost:3000` by default. Update in `src/api.rs` for production.

---

::: tip Next Steps
- Learn about [WebSocket Events](/api/websocket)
- Read the [Configuration Guide](/guide/configuration)
:::
