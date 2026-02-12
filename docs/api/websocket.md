# WebSocket Events

SOL BEAST uses WebSocket for real-time updates to the frontend dashboard.

## Connection

Connect to the WebSocket server:

```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

ws.onopen = () => {
  console.log('Connected to SOL BEAST');
};

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  handleEvent(data);
};

ws.onerror = (error) => {
  console.error('WebSocket error:', error);
};

ws.onclose = () => {
  console.log('Disconnected from SOL BEAST');
  // Implement reconnection logic
};
```

## Event Types

### Bot Status Update

Sent when bot status changes.

```json
{
  "type": "status_update",
  "data": {
    "running": true,
    "mode": "dry",
    "connected": true,
    "uptime": 123
  }
}
```

### New Event Detected

Sent when a pump.fun event is detected.

```json
{
  "type": "event_detected",
  "data": {
    "mint": "TokenMintAddress123...",
    "type": "token_created",
    "timestamp": "2024-01-15T10:30:00Z"
  }
}
```

### Buy Signal

Sent when heuristics pass and bot would/does buy.

```json
{
  "type": "buy_signal",
  "data": {
    "mint": "TokenMintAddress123...",
    "reason": "heuristics_passed",
    "liquidity_sol": 15.5,
    "holder_count": 75,
    "token_age_seconds": 120,
    "executed": false
  }
}
```

### Trade Executed

Sent when a trade is executed (buy or sell).

```json
{
  "type": "trade_executed",
  "data": {
    "type": "buy",
    "mint": "TokenMintAddress123...",
    "amount_sol": 0.05,
    "amount_tokens": 1000000,
    "price": 0.00005,
    "signature": "TransactionSignature...",
    "timestamp": "2024-01-15T10:30:00Z"
  }
}
```

### Holding Update

Sent when a holding's price or status changes.

```json
{
  "type": "holding_update",
  "data": {
    "mint": "TokenMintAddress123...",
    "current_price": 0.00006,
    "pnl_percentage": 20.0,
    "pnl_sol": 0.01,
    "age_seconds": 120
  }
}
```

### Position Closed

Sent when a position is closed (TP/SL/timeout).

```json
{
  "type": "position_closed",
  "data": {
    "mint": "TokenMintAddress123...",
    "exit_reason": "take_profit",
    "entry_price": 0.00005,
    "exit_price": 0.000075,
    "pnl_sol": 0.025,
    "pnl_percentage": 50.0,
    "hold_time_seconds": 180,
    "signature": "TransactionSignature..."
  }
}
```

### Error Event

Sent when an error occurs.

```json
{
  "type": "error",
  "data": {
    "message": "Failed to execute trade",
    "code": "TRANSACTION_FAILED",
    "mint": "TokenMintAddress123...",
    "timestamp": "2024-01-15T10:30:00Z"
  }
}
```

## Exit Reasons

Possible values for `exit_reason` in position_closed events:

- `take_profit` - Price reached TP target
- `stop_loss` - Price hit SL target
- `timeout` - Position held for timeout duration
- `manual` - Manually closed by user
- `error` - Closed due to error

## Client Example (React)

```typescript
import { useEffect, useState } from 'react';

interface WSMessage {
  type: string;
  data: any;
}

export function useWebSocket() {
  const [ws, setWs] = useState<WebSocket | null>(null);
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    const socket = new WebSocket('ws://localhost:8080/ws');

    socket.onopen = () => {
      setConnected(true);
      console.log('WebSocket connected');
    };

    socket.onmessage = (event) => {
      const message: WSMessage = JSON.parse(event.data);
      handleMessage(message);
    };

    socket.onerror = (error) => {
      console.error('WebSocket error:', error);
    };

    socket.onclose = () => {
      setConnected(false);
      console.log('WebSocket closed');
      // Reconnect after 3 seconds
      setTimeout(() => {
        setWs(null);
      }, 3000);
    };

    setWs(socket);

    return () => {
      socket.close();
    };
  }, []);

  const handleMessage = (message: WSMessage) => {
    switch (message.type) {
      case 'trade_executed':
        // Handle trade
        break;
      case 'holding_update':
        // Update holdings UI
        break;
      // ... other cases
    }
  };

  return { ws, connected };
}
```

## Reconnection

Implement exponential backoff for reconnection:

```javascript
let reconnectDelay = 1000;
const maxReconnectDelay = 30000;

function connect() {
  const ws = new WebSocket('ws://localhost:8080/ws');

  ws.onopen = () => {
    reconnectDelay = 1000; // Reset on successful connection
  };

  ws.onclose = () => {
    setTimeout(() => {
      reconnectDelay = Math.min(reconnectDelay * 2, maxReconnectDelay);
      connect();
    }, reconnectDelay);
  };
}
```

---

::: tip See Also
- [REST API Endpoints](/api/endpoints)
- [Frontend Integration](/guide/dashboard)
:::
