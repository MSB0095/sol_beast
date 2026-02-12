# Installation

Detailed installation instructions for SOL BEAST across different platforms.

## System Requirements

### Minimum Requirements
- **CPU**: 2 cores
- **RAM**: 4 GB
- **Disk**: 2 GB free space
- **OS**: Linux, macOS, or Windows 10+
- **Network**: Stable internet connection (low latency preferred)

### Recommended Requirements
- **CPU**: 4+ cores
- **RAM**: 8+ GB
- **Disk**: 10+ GB SSD
- **OS**: Linux (Ubuntu 20.04+) or macOS
- **Network**: Dedicated connection with < 50ms latency to Solana mainnet

## Installing Rust

### Linux / macOS

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
rustc --version
```

### Windows

1. Download and install from [rustup.rs](https://rustup.rs/)
2. Follow the installer instructions
3. Restart your terminal
4. Verify: `rustc --version`

## Installing Node.js

### Linux (Ubuntu/Debian)

```bash
curl -fsSL https://deb.nodesource.com/setup_18.x | sudo -E bash -
sudo apt-get install -y nodejs
node --version
npm --version
```

### macOS (Homebrew)

```bash
brew install node@18
node --version
npm --version
```

### Windows

1. Download installer from [nodejs.org](https://nodejs.org/)
2. Run the installer
3. Verify: `node --version` and `npm --version`

## Installing Solana CLI (Optional)

### Linux / macOS

```bash
sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
solana --version
```

### Windows (PowerShell)

```powershell
cmd /c "curl https://release.solana.com/stable/solana-install-init-x86_64-pc-windows-msvc.exe --output solana-install-tmp.exe --create-dirs"
.\solana-install-tmp.exe
```

## Installing SOL BEAST

### Option 1: From Source (Recommended)

```bash
# Clone repository
git clone https://github.com/MSB0095/sol_beast.git
cd sol_beast

# Build backend
cargo build --release

# Install frontend dependencies
cd frontend
npm install
cd ..

# Copy configuration
cp config.example.toml config.toml
```

### Option 2: Using Pre-built Binaries (Coming Soon)

Pre-built binaries will be available for:
- Linux (x86_64)
- macOS (Intel & ARM)
- Windows (x86_64)

## Post-Installation Setup

### 1. Create Trading Wallet

```bash
# Generate new keypair
solana-keygen new --outfile ~/.config/solana/sol-beast-wallet.json

# Secure the file
chmod 600 ~/.config/solana/sol-beast-wallet.json

# Get the public address
solana-keygen pubkey ~/.config/solana/sol-beast-wallet.json
```

::: warning Security
Store your keypair securely. Consider using a hardware wallet for larger amounts.
:::

### 2. Fund Wallet

Transfer SOL to your trading wallet address:
- **Minimum**: 0.5 SOL (for testing with small trades)
- **Recommended**: 2-5 SOL (for comfortable trading)

### 3. Configure RPC Endpoint

Get a free RPC endpoint from:
- [Helius](https://helius.dev/) - Recommended, free tier available
- [QuickNode](https://www.quicknode.com/)
- Or use public RPC (slower)

### 4. Edit Configuration

```bash
nano config.toml
```

Set minimum required fields:
```toml
wallet_keypair_path = "/home/user/.config/solana/sol-beast-wallet.json"
solana_rpc_urls = ["https://rpc.helius.xyz/?api-key=YOUR_API_KEY"]
```

## Verification

### Test Backend Build

```bash
cargo build --release
./target/release/sol_beast --help
```

Expected output:
```
sol_beast 0.1.0
Usage: sol_beast [OPTIONS]

Options:
      --real    Enable real trading mode
  -h, --help    Print help
```

### Test Frontend Build

```bash
cd frontend
npm run build
```

Should complete without errors.

### Test Dry Run

```bash
RUST_LOG=info cargo run
```

You should see:
```
INFO Starting sol_beast in DRY MODE
INFO WebSocket connected
```

## Platform-Specific Notes

### Linux

**Ubuntu/Debian Dependencies:**
```bash
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev
```

**Arch Linux:**
```bash
sudo pacman -S base-devel openssl
```

### macOS

**Install Xcode Command Line Tools:**
```bash
xcode-select --install
```

**M1/M2 Macs:**
- All dependencies should work natively
- No Rosetta required

### Windows

**Visual Studio Build Tools:**
- Download from [Microsoft](https://visualstudio.microsoft.com/downloads/)
- Select "Desktop development with C++"

**PowerShell Execution Policy:**
```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
```

## Troubleshooting Installation

### Rust Build Errors

**Error**: `linker 'cc' not found`
- **Solution**: Install build tools for your platform

**Error**: `could not find native static library 'ssl'`
- **Linux**: `sudo apt install libssl-dev`
- **macOS**: `brew install openssl`

### Node.js Issues

**Error**: `EACCES: permission denied`
- **Solution**: Fix npm permissions:
```bash
mkdir ~/.npm-global
npm config set prefix '~/.npm-global'
export PATH=~/.npm-global/bin:$PATH
```

### Frontend Build Issues

**Error**: `Module not found`
- **Solution**: Delete and reinstall:
```bash
rm -rf node_modules package-lock.json
npm install
```

## Docker Installation (Alternative)

Coming soon: Docker and Docker Compose support for easier deployment.

## Next Steps

After successful installation:
1. [Configure the bot](/guide/configuration)
2. [Run your first test](/guide/getting-started#first-run-dry-mode)
3. [Set up the dashboard](/guide/dashboard)

---

::: info Need Help?
Having installation issues? Check the [Troubleshooting Guide](/guide/troubleshooting) or [open an issue](https://github.com/MSB0095/sol_beast/issues).
:::
