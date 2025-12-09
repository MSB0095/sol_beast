#!/bin/bash

# Sol Beast - Unified Start Script
# Supports both CLI+Frontend and WASM+Frontend modes

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Function to display usage
usage() {
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║                      Sol Beast Launcher                         ║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${YELLOW}Usage:${NC} $0 [mode] [options]"
    echo ""
    echo -e "${YELLOW}Available modes:${NC}"
    echo -e "  ${GREEN}cli${NC}      - Start CLI backend + Frontend (default)"
    echo -e "  ${GREEN}wasm${NC}     - Build WASM + Start Frontend (browser-only mode)"
    echo -e "  ${GREEN}help${NC}     - Show this help message"
    echo ""
    echo -e "${YELLOW}Options:${NC}"
    echo -e "  ${GREEN}--tunnel${NC} - Enable remote access via localtunnel (public HTTPS URL)"
    echo ""
    echo -e "${YELLOW}Examples:${NC}"
    echo -e "  $0                ${BLUE}# Starts in CLI mode${NC}"
    echo -e "  $0 cli            ${BLUE}# Starts CLI backend + Frontend${NC}"
    echo -e "  $0 cli --tunnel   ${BLUE}# Starts with public remote access${NC}"
    echo -e "  $0 wasm --tunnel  ${BLUE}# Starts WASM mode with remote access${NC}"
    echo ""
    echo -e "${CYAN}For password-free tunnel access, see:${NC} ./scripts/tunnel-auto.sh"
    echo ""
    exit 0
}

# Function to check if command exists
command_exists() {
    command -v "$1" &> /dev/null
}

# Function to check prerequisites
check_prerequisites() {
    echo -e "${BLUE}🔍 Checking prerequisites...${NC}"
    
    local missing_deps=()
    
    if ! command_exists cargo; then
        missing_deps+=("cargo (Rust)")
    fi
    
    if ! command_exists npm; then
        missing_deps+=("npm (Node.js)")
    fi
    
    if [ ${#missing_deps[@]} -ne 0 ]; then
        echo -e "${RED}❌ Missing dependencies:${NC}"
        for dep in "${missing_deps[@]}"; do
            echo -e "   - $dep"
        done
        echo ""
        echo -e "${YELLOW}Please install missing dependencies and try again.${NC}"
        exit 1
    fi
    
    echo -e "${GREEN}✓ All prerequisites met${NC}"
    echo ""
}

# Function to install frontend dependencies if needed
check_frontend_deps() {
    if [ ! -d "$SCRIPT_DIR/frontend/node_modules" ]; then
        echo -e "${YELLOW}📦 Installing frontend dependencies...${NC}"
        cd "$SCRIPT_DIR/frontend"
        npm install
        cd "$SCRIPT_DIR"
        echo -e "${GREEN}✓ Frontend dependencies installed${NC}"
        echo ""
    fi
}

# Function to check if localtunnel is installed
check_tunnel_deps() {
    if ! command_exists lt; then
        echo -e "${YELLOW}📦 Installing localtunnel...${NC}"
        cd "$SCRIPT_DIR/frontend"
        npm install
        cd "$SCRIPT_DIR"
        echo -e "${GREEN}✓ localtunnel installed${NC}"
        echo ""
    fi
}

# Function to start tunnel
start_tunnel() {
    local port=${1:-3000}
    
    echo -e "${BLUE}🌐 Starting localtunnel...${NC}"
    echo -e "${YELLOW}⚠️  Security Warning: Your frontend will be publicly accessible!${NC}"
    echo -e "${YELLOW}    API has no authentication - use with caution!${NC}"
    echo -e "${BLUE}    Using random URL to support multiple concurrent users...${NC}"
    echo ""
    
    cd "$SCRIPT_DIR/frontend"
    npx localtunnel --port "$port" > /tmp/sol_beast_tunnel.log 2>&1 &
    TUNNEL_PID=$!
    cd "$SCRIPT_DIR"
    
    # Wait for tunnel to establish
    echo -e "${YELLOW}⏳ Establishing tunnel...${NC}"
    sleep 3
    
    # Try to extract the URL from the log
    local tunnel_url=""
    for i in {1..10}; do
        if [ -f /tmp/sol_beast_tunnel.log ]; then
            tunnel_url=$(grep -oP 'https://[^\s]+' /tmp/sol_beast_tunnel.log | head -1)
            if [ ! -z "$tunnel_url" ]; then
                break
            fi
        fi
        sleep 1
    done
    
    if [ ! -z "$tunnel_url" ]; then
        # Get tunnel password (public IP)
        local tunnel_password=$(curl -s https://loca.lt/mytunnelpassword 2>/dev/null || curl -s ifconfig.me 2>/dev/null || echo "unknown")
        
        echo -e "${GREEN}✓ Tunnel established${NC}"
        echo ""
        echo -e "${GREEN}╔══════════════════════════════════════════════════════════════════╗${NC}"
        echo -e "${GREEN}║                  🌍 PUBLIC ACCESS ENABLED 🌍                     ║${NC}"
        echo -e "${GREEN}╚══════════════════════════════════════════════════════════════════╝${NC}"
        echo ""
        echo -e "${CYAN}🔗 Tunnel URL:${NC}      ${GREEN}${tunnel_url}${NC}"
        echo -e "${CYAN}🔑 Password:${NC}        ${YELLOW}${tunnel_password}${NC}"
        echo ""
        echo -e "${YELLOW}📋 Share with visitors:${NC}"
        echo -e "   ${GREEN}URL:${NC}      ${tunnel_url}"
        echo -e "   ${GREEN}Password:${NC} ${tunnel_password}"
        echo ""
        echo -e "${BLUE}💡 Visitors will be asked for the password once (valid 7 days)${NC}"
        echo ""
        echo -e "${YELLOW}⚡ For NO password option, use ngrok instead:${NC}"
        echo -e "   ./scripts/ngrok-tunnel.sh"
        echo ""
        echo -e "${RED}⚠️  Security: No authentication is enabled on the API!${NC}"
        echo ""
    else
        echo -e "${YELLOW}⚠️  Tunnel started but URL not detected yet${NC}"
        echo -e "${YELLOW}    Check logs at: /tmp/sol_beast_tunnel.log${NC}"
        echo -e "${YELLOW}    Note: Subdomain '$subdomain' may be taken, trying random URL${NC}"
        echo ""
    fi
}

# Function to cleanup background processes on exit
cleanup() {
    echo ""
    echo -e "${YELLOW}🛑 Shutting down...${NC}"
    
    if [ ! -z "$TUNNEL_PID" ]; then
        echo -e "${BLUE}Stopping tunnel (PID: $TUNNEL_PID)...${NC}"
        kill $TUNNEL_PID 2>/dev/null || true
    fi
    
    if [ ! -z "$BACKEND_PID" ]; then
        echo -e "${BLUE}Stopping backend (PID: $BACKEND_PID)...${NC}"
        kill $BACKEND_PID 2>/dev/null || true
    fi
    
    if [ ! -z "$FRONTEND_PID" ]; then
        echo -e "${BLUE}Stopping frontend (PID: $FRONTEND_PID)...${NC}"
        kill $FRONTEND_PID 2>/dev/null || true
    fi
    
    # Kill any remaining processes on the ports
    lsof -ti:8080 | xargs kill -9 2>/dev/null || true
    lsof -ti:3000 | xargs kill -9 2>/dev/null || true
    
    echo -e "${GREEN}✓ Cleanup complete${NC}"
    exit 0
}

# Set up trap for cleanup
trap cleanup EXIT INT TERM

# Function to start CLI mode
start_cli_mode() {
    local enable_tunnel=$1
    
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║        Starting Sol Beast - CLI Mode (Webpack + WASM)           ║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    
    check_prerequisites
    check_frontend_deps
    
    if [ "$enable_tunnel" = "true" ]; then
        check_tunnel_deps
    fi
    
    # Check if config exists
    if [ ! -f "$SCRIPT_DIR/config.toml" ]; then
        if [ -f "$SCRIPT_DIR/config.example.toml" ]; then
            echo -e "${YELLOW}⚠️  config.toml not found. Please copy config.example.toml to config.toml and configure it.${NC}"
            echo -e "${BLUE}   cp config.example.toml config.toml${NC}"
            echo ""
            exit 1
        fi
    fi
    
    # Start backend in background
    echo -e "${BLUE}🚀 Starting CLI backend...${NC}"
    echo -e "${BLUE}   Backend URL: http://localhost:8080${NC}"
    echo -e "${BLUE}   API: http://localhost:8080/api${NC}"
    echo ""
    
    export RUST_LOG=info
    cargo run --release > /tmp/sol_beast_backend.log 2>&1 &
    BACKEND_PID=$!
    
    # Wait for backend to be ready
    echo -e "${YELLOW}⏳ Waiting for backend to start...${NC}"
    for i in {1..30}; do
        if curl -s http://localhost:8080 > /dev/null 2>&1; then
            echo -e "${GREEN}✓ Backend is ready${NC}"
            echo ""
            break
        fi
        sleep 1
        if [ $i -eq 30 ]; then
            echo -e "${RED}❌ Backend failed to start. Check logs at /tmp/sol_beast_backend.log${NC}"
            exit 1
        fi
    done
    
    # Start frontend
    echo -e "${BLUE}🚀 Starting frontend...${NC}"
    echo -e "${BLUE}   Frontend URL: http://localhost:3000${NC}"
    echo ""
    
    cd "$SCRIPT_DIR/frontend"
    npm run dev > /tmp/sol_beast_frontend.log 2>&1 &
    FRONTEND_PID=$!
    
    # Wait for frontend to be ready
    echo -e "${YELLOW}⏳ Waiting for frontend to start...${NC}"
    for i in {1..30}; do
        if curl -s http://localhost:3000 > /dev/null 2>&1; then
            echo -e "${GREEN}✓ Frontend is ready${NC}"
            echo ""
            break
        fi
        sleep 1
    done
    
    # Start tunnel if enabled
    if [ "$enable_tunnel" = "true" ]; then
        start_tunnel 3000 "solbeast"
    fi
    
    echo -e "${GREEN}╔══════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║                Sol Beast is Running! 🚀                          ║${NC}"
    echo -e "${GREEN}╚══════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${CYAN}📍 Frontend:${NC}  http://localhost:3000"
    echo -e "${CYAN}📍 Backend:${NC}   http://localhost:8080"
    echo -e "${CYAN}📍 API:${NC}       http://localhost:8080/api"
    echo ""
    echo -e "${YELLOW}📋 Logs:${NC}"
    echo -e "   Backend:  /tmp/sol_beast_backend.log"
    echo -e "   Frontend: /tmp/sol_beast_frontend.log"
    if [ "$enable_tunnel" = "true" ]; then
        echo -e "   Tunnel:   /tmp/sol_beast_tunnel.log"
    fi
    echo ""
    echo -e "${RED}Press Ctrl+C to stop all services${NC}"
    echo ""
    
    # Keep script running and show logs
    if [ "$enable_tunnel" = "true" ]; then
        tail -f /tmp/sol_beast_backend.log /tmp/sol_beast_frontend.log /tmp/sol_beast_tunnel.log
    else
        tail -f /tmp/sol_beast_backend.log /tmp/sol_beast_frontend.log
    fi
}

# Function to start WASM mode
start_wasm_mode() {
    local enable_tunnel=$1
    
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║              Starting Sol Beast - WASM Mode                      ║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    
    check_prerequisites
    check_frontend_deps
    
    if [ "$enable_tunnel" = "true" ]; then
        check_tunnel_deps
    fi
    
    # Check if wasm-pack is installed
    if ! command_exists wasm-pack; then
        echo -e "${YELLOW}📦 Installing wasm-pack...${NC}"
        curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
        echo -e "${GREEN}✓ wasm-pack installed${NC}"
        echo ""
    fi
    
    # Build WASM
    echo -e "${BLUE}🔨 Building WASM module...${NC}"
    echo -e "${BLUE}   This may take a few minutes on first build...${NC}"
    echo ""
    
    if ./build-wasm.sh; then
        echo -e "${GREEN}✓ WASM build complete${NC}"
        echo ""
    else
        echo -e "${RED}❌ WASM build failed${NC}"
        exit 1
    fi
    
    # Start frontend
    echo -e "${BLUE}🚀 Starting frontend in WASM mode...${NC}"
    echo -e "${BLUE}   Frontend URL: http://localhost:3000${NC}"
    echo -e "${BLUE}   Mode: Browser-only (no backend required)${NC}"
    echo ""
    
    cd "$SCRIPT_DIR/frontend"
    VITE_USE_WASM=true npm run dev > /tmp/sol_beast_frontend.log 2>&1 &
    FRONTEND_PID=$!
    
    # Wait for frontend to be ready
    echo -e "${YELLOW}⏳ Waiting for frontend to start...${NC}"
    for i in {1..30}; do
        if curl -s http://localhost:3000 > /dev/null 2>&1; then
            echo -e "${GREEN}✓ Frontend is ready${NC}"
            echo ""
            break
        fi
        sleep 1
    done
    
    # Start tunnel if enabled
    if [ "$enable_tunnel" = "true" ]; then
        start_tunnel 3000 "solbeast"
    fi
    
    echo -e "${GREEN}╔══════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║          Sol Beast is Running in WASM Mode! 🚀                  ║${NC}"
    echo -e "${GREEN}╚══════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${CYAN}📍 Frontend:${NC}  http://localhost:3000"
    echo -e "${CYAN}🌐 Mode:${NC}      Browser-only (WASM)"
    echo ""
    echo -e "${YELLOW}📋 Logs:${NC}"
    echo -e "   Frontend: /tmp/sol_beast_frontend.log"
    if [ "$enable_tunnel" = "true" ]; then
        echo -e "   Tunnel:   /tmp/sol_beast_tunnel.log"
    fi
    echo ""
    echo -e "${RED}Press Ctrl+C to stop${NC}"
    echo ""
    
    # Keep script running and show logs
    if [ "$enable_tunnel" = "true" ]; then
        tail -f /tmp/sol_beast_frontend.log /tmp/sol_beast_tunnel.log
    else
        tail -f /tmp/sol_beast_frontend.log
    fi
}

# Main script logic
MODE="${1:-cli}"
ENABLE_TUNNEL="false"

# Parse arguments
if [ "$2" = "--tunnel" ] || [ "$1" = "--tunnel" ]; then
    ENABLE_TUNNEL="true"
    if [ "$1" = "--tunnel" ]; then
        MODE="cli"
    fi
fi

case "$MODE" in
    cli)
        start_cli_mode "$ENABLE_TUNNEL"
        ;;
    wasm)
        start_wasm_mode "$ENABLE_TUNNEL"
        ;;
    help|--help|-h)
        usage
        ;;
    *)
        echo -e "${RED}❌ Invalid mode: $MODE${NC}"
        echo ""
        usage
        ;;
esac
