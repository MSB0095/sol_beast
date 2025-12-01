# Sol Beast - Dual-Mode Deployment Guide

## 🎯 Overview

Sol Beast now supports **two deployment modes**:

1. **WASM Mode** (GitHub Pages) - Frontend + WASM, no backend needed
2. **Backend Mode** (Local) - Frontend + Rust backend (original)

## 🚀 Quick Start

### Option 1: GitHub Pages (WASM Mode)

**Fully browser-based, no server required!**

```bash
# Build WASM module
./build-wasm.sh

# Build frontend
cd frontend
npm install
VITE_USE_WASM=true npm run build

# Deploy dist/ to GitHub Pages
```

**Automatic deployment**: Push to `main` branch and GitHub Actions handles everything.

### Option 2: Local Hosting (Backend Mode)

**Full-featured with backend server**

Terminal 1 - Backend:
```bash
# Build and run backend
cargo build --release --package sol_beast_cli
./target/release/sol_beast
```

Terminal 2 - Frontend:
```bash
cd frontend
npm install
npm run dev
```

## 📦 Building

### Build WASM Only
```bash
./build-wasm.sh
```
Output: `frontend/src/wasm/`

### Build Backend Only
```bash
cargo build --release --package sol_beast_cli
```
Output: `target/release/sol_beast`

### Build Frontend Only
```bash
cd frontend
npm run build:frontend-only
```

### Build Everything
```bash
# WASM + Frontend
./build-wasm.sh
cd frontend
npm run build
```

## 🔧 Configuration

### WASM Mode
Controlled by environment variable:
```bash
export VITE_USE_WASM=true
npm run build
```

Or auto-detected on GitHub Pages (*.github.io domains).

### Backend Mode
Default when running locally:
```bash
npm run dev  # Uses localhost:8080 API
```

## 🏗️ Architecture

### WASM Mode
```
┌─────────────────────┐
│   Browser           │
│                     │
│  React Frontend     │
│        ↓            │
│  WASM Bot Module    │
│        ↓            │
│  Solana RPC/WS      │
└─────────────────────┘
```

### Backend Mode
```
┌─────────┐    HTTP    ┌──────────────┐
│ Browser │ ←────────→ │ Rust Backend │
│ (React) │            │  (Axum API)  │
└─────────┘            └──────┬───────┘
                              ↓
                         Solana RPC/WS
```

## 📁 Project Structure

```
sol_beast/
├── sol_beast_core/      # Shared logic (both modes)
├── sol_beast_wasm/      # WASM bindings (browser)
├── sol_beast_cli/       # Backend server (native)
├── frontend/
│   ├── src/
│   │   ├── services/
│   │   │   └── botService.ts  # Dual-mode adapter
│   │   └── wasm/          # Generated WASM (git-ignored)
│   └── package.json
├── build-wasm.sh        # WASM build script
└── .github/workflows/
    └── deploy.yml       # GitHub Pages deployment
```

## 🔍 Detection Logic

The frontend automatically detects which mode to use:

```typescript
// Auto-detect in botService.ts
const USE_WASM = 
  import.meta.env.VITE_USE_WASM === 'true' ||  // Manual override
  window.location.hostname.includes('github.io') // GitHub Pages
```

## 🛠️ Development

### Test WASM Mode Locally
```bash
# Build WASM
./build-wasm.sh

# Run frontend with WASM
cd frontend
VITE_USE_WASM=true npm run dev
```

### Test Backend Mode Locally
```bash
# Terminal 1: Backend
cargo run --package sol_beast_cli

# Terminal 2: Frontend
cd frontend
npm run dev
```

## 📊 Feature Comparison

| Feature | WASM Mode | Backend Mode |
|---------|-----------|--------------|
| **Server required** | ❌ No | ✅ Yes |
| **GitHub Pages** | ✅ Yes | ❌ No |
| **Trading** | ✅ Yes | ✅ Yes |
| **Performance** | 🟡 Good | ✅ Excellent |
| **RPC Limits** | 🟡 Browser CORS | ✅ No limits |
| **WebSocket** | ✅ Yes | ✅ Yes |
| **Private Keys** | 🟡 Browser storage | ✅ Server filesystem |
| **Recommended for** | Casual users | Production trading |

## 🚨 Security Notes

### WASM Mode
- Keys stored in browser localStorage
- Vulnerable to XSS attacks
- Use for testing/casual trading only

### Backend Mode
- Keys on server filesystem
- More secure isolation
- Recommended for production

## 🐛 Troubleshooting

### WASM build fails
```bash
# Install wasm-pack
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Add WASM target
rustup target add wasm32-unknown-unknown

# Try again
./build-wasm.sh
```

### Frontend can't find WASM
```bash
# Ensure WASM was built
ls frontend/src/wasm/

# Should see:
# - sol_beast_wasm.js
# - sol_beast_wasm_bg.wasm
# - package.json
```

### Backend mode not working
```bash
# Check backend is running
curl http://localhost:8080/health

# If not running, start it
cargo run --package sol_beast_cli
```

## 📝 Environment Variables

### Frontend
- `VITE_USE_WASM` - Force WASM mode (`true`/`false`)
- `VITE_API_BASE_URL` - Backend URL (default: `http://localhost:8080`)

### Backend
- `RUST_LOG` - Log level (`info`, `debug`, `warn`)
- `SOL_BEAST_CONFIG_PATH` - Config file path (default: `config.toml`)

## 🎓 Examples

### Deploy to GitHub Pages
1. Push to `main` branch
2. GitHub Actions builds WASM
3. Deploys to `https://yourusername.github.io/sol_beast/`
4. Users access fully functional bot in browser!

### Self-host with Backend
1. Rent a VPS (DigitalOcean, AWS, etc.)
2. Build: `cargo build --release --package sol_beast_cli`
3. Run: `./target/release/sol_beast --real`
4. Deploy frontend: `npm run build` → serve `dist/`
5. Point frontend to your backend URL

## 📖 Documentation

- **WASM_PROGRESS.md** - Detailed implementation roadmap
- **WASM_STATUS.md** - Current implementation status
- **DEPLOYMENT.md** - Original deployment guide
- **FEATURES.md** - Feature list and usage

## 💡 Tips

1. **Start with WASM mode** for testing (no server setup)
2. **Upgrade to backend mode** for production trading
3. **Use dry-run mode** first in both cases
4. **Monitor RPC usage** in WASM mode (rate limits apply)
5. **Backup your keys** regardless of mode

## 🔄 Migration

### From Backend to WASM
1. Export settings from backend
2. Build WASM: `./build-wasm.sh`
3. Set `VITE_USE_WASM=true`
4. Import settings in browser

### From WASM to Backend
1. Download settings from browser localStorage
2. Create `config.toml` with settings
3. Start backend with settings
4. Connect frontend to backend URL

---

**Ready to trade?** Choose your mode and get started! 🚀
