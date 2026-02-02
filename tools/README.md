# Sol Beast Tools & Scripts

This directory contains development and utility scripts for Sol Beast.

## Quick Start

From the project root, simply run:
```bash
./start.sh
```

This automatically detects your OS and starts both backend and frontend services.

## Individual Scripts

### `backend.sh`
Start the Rust backend service on port 8080.
```bash
bash tools/backend.sh
```

### `frontend.sh`
Start the React frontend development server on port 3000.
```bash
bash tools/frontend.sh
```

### `pumpportal_listen.py`
Development debugging tool to monitor PumpPortal WebSocket stream (15-second snapshot).

**Usage:**
```bash
python3 tools/pumpportal_listen.py
```

**Note:** This is a standalone debugging utility. It is NOT required for normal operation and is primarily used during development to inspect raw PumpPortal data format.

## Cross-Platform Behavior

The `start.sh` script automatically adapts to your OS:

- **macOS**: Opens Terminal.app windows for backend and frontend
- **Linux**: Uses available terminal (gnome-terminal, xterm, konsole) or runs in background
- **Windows**: Uses Command Prompt or runs in background

If no native terminal is available, services run as background processes with Ctrl+C instructions.

## Troubleshooting

### Backend won't start
```bash
# Check if port 8080 is in use
lsof -i :8080  # macOS/Linux
netstat -ano | findstr :8080  # Windows

# Kill the process if needed
kill -9 <PID>  # macOS/Linux
taskkill /PID <PID> /F  # Windows
```

### Frontend won't start
```bash
# Check if port 3000 is in use
lsof -i :3000  # macOS/Linux

# Ensure npm dependencies are installed
cd frontend && npm install
```

### Services running in background won't stop
```bash
# Kill all Sol Beast processes
pkill -f 'cargo run|npm run dev'
```

## Production Deployment

For production, consider:
- **Docker**: See ../DEPLOYMENT.md
- **Systemd**: Create service files for Linux systems
- **PM2**: Use PM2 process manager for Node.js frontend
- **Cargo release**: Pre-compile backend with `cargo build --release`
