# Sol Beast Frontend

A comprehensive React + TypeScript dashboard for the Sol Beast Solana trading bot.

## Features

- **Real-time Dashboard**: Live monitoring of trading activity, profits, and bot statistics
- **Full Configuration Management**: Edit all bot settings directly from the UI
- **Holdings Tracker**: Monitor active positions with real-time profit/loss calculations
- **WebSocket Support**: Real-time updates via WebSocket (ready for integration)
- **Responsive Design**: Works on desktop and tablet devices
- **Dark Theme**: Optimized for extended viewing sessions

## Installation

```bash
cd frontend
npm install
```

## Development

```bash
npm run dev
```

The frontend will start on `http://localhost:3000` and proxy API requests to `http://localhost:8080/api`.

## Building

```bash
npm run build
```

## Architecture

### Components
- **Header**: Navigation and connection status
- **Dashboard**: Overview of trading performance and statistics
- **ConfigurationPanel**: Full settings editor with categorized options
- **HoldingsPanel**: Table view of active positions with P/L tracking

### State Management
Uses Zustand for lightweight state management:
- **botStore**: Bot connection status and statistics
- **settingsStore**: Configuration management and UI state

### Styling
- **Tailwind CSS**: Utility-first styling
- **Custom theme**: Solana-inspired purple and dark colors
- **Responsive**: Mobile-first approach

## API Endpoints

The frontend communicates with the backend via these endpoints:

### Health & Status
```
GET /api/health
GET /api/stats
```

### Settings Management
```
GET /api/settings       # Fetch current settings
POST /api/settings      # Update settings (JSON body)
```

## Environment Variables

Configure the backend URL by setting the API proxy in `vite.config.ts`:

```typescript
proxy: {
  '/api': {
    target: 'http://localhost:8080',
    changeOrigin: true,
  }
}
```

## Keyboard Shortcuts

- `Ctrl+1` / `Cmd+1`: Go to Dashboard
- `Ctrl+2` / `Cmd+2`: Go to Configuration
- `Ctrl+3` / `Cmd+3`: Go to Holdings

## Future Enhancements

- [ ] WebSocket real-time updates
- [ ] Order history and detailed trading logs
- [ ] Advanced charting with TradingView
- [ ] Multi-wallet support
- [ ] Transaction signing UI
- [ ] Export reports (CSV/PDF)
- [ ] Dark/Light theme toggle
- [ ] Mobile app (React Native)

## Browser Support

- Chrome/Edge 90+
- Firefox 88+
- Safari 14+

## License

MIT
