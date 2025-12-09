#!/bin/bash

# Sol Beast Tunnel with Auto-Bypass
# Automatically starts tunnel + bypass proxy

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# Default values
PORT=3000
PROXY_PORT=8888

# Function to check if command exists
command_exists() {
    command -v "$1" &> /dev/null
}

# Check if port is accessible
echo -e "${BLUE}🔍 Checking if port $PORT is accessible...${NC}"
if ! curl -s http://localhost:$PORT > /dev/null 2>&1; then
    echo -e "${RED}❌ Cannot connect to localhost:$PORT${NC}"
    echo -e "${YELLOW}   Make sure Sol Beast is running first!${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Port $PORT is accessible${NC}"
echo ""

# Function to cleanup on exit
cleanup() {
    echo ""
    echo -e "${YELLOW}🛑 Shutting down...${NC}"
    if [ ! -z "$TUNNEL_PID" ]; then
        echo -e "${BLUE}Stopping tunnel...${NC}"
        kill $TUNNEL_PID 2>/dev/null || true
    fi
    if [ ! -z "$PROXY_PID" ]; then
        echo -e "${BLUE}Stopping bypass proxy...${NC}"
        kill $PROXY_PID 2>/dev/null || true
    fi
    echo -e "${GREEN}✓ Cleanup complete${NC}"
    exit 0
}

trap cleanup EXIT INT TERM

# Start tunnel
echo -e "${BLUE}🌐 Starting localtunnel...${NC}"
echo -e "${BLUE}    Using random URL to support multiple concurrent users...${NC}"
cd "$PROJECT_DIR/frontend"
npx localtunnel --port "$PORT" > /tmp/sol_beast_tunnel_auto.log 2>&1 &
TUNNEL_PID=$!
cd "$PROJECT_DIR"

# Wait for tunnel and get URL
echo -e "${YELLOW}⏳ Waiting for tunnel to establish...${NC}"
sleep 3

TUNNEL_URL=""
for i in {1..15}; do
    if [ -f /tmp/sol_beast_tunnel_auto.log ]; then
        TUNNEL_URL=$(grep -oP 'https://[^\s]+' /tmp/sol_beast_tunnel_auto.log | head -1)
        if [ ! -z "$TUNNEL_URL" ]; then
            break
        fi
    fi
    sleep 1
done

if [ -z "$TUNNEL_URL" ]; then
    echo -e "${RED}❌ Could not establish tunnel${NC}"
    echo -e "${YELLOW}Check logs: /tmp/sol_beast_tunnel_auto.log${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Tunnel established: ${TUNNEL_URL}${NC}"
echo ""

# Start bypass proxy
echo -e "${BLUE}🔓 Starting bypass proxy...${NC}"
node "$SCRIPT_DIR/tunnel-proxy.js" "$TUNNEL_URL" "$PROXY_PORT" > /tmp/sol_beast_proxy.log 2>&1 &
PROXY_PID=$!

# Wait for proxy to start
sleep 2

# Get public IP for sharing
PUBLIC_IP=$(curl -s ifconfig.me 2>/dev/null || echo "YOUR_IP")

echo -e "${GREEN}╔══════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║           🌍 PUBLIC ACCESS WITH BYPASS ACTIVE 🌍                 ║${NC}"
echo -e "${GREEN}╚══════════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${CYAN}🔗 Tunnel URL (with password):${NC}"
echo -e "   ${TUNNEL_URL}"
echo ""
echo -e "${CYAN}✨ Bypass URL (NO password needed):${NC}"
echo -e "   ${GREEN}http://localhost:${PROXY_PORT}${NC} ${BLUE}(local access)${NC}"
echo -e "   ${GREEN}http://${PUBLIC_IP}:${PROXY_PORT}${NC} ${BLUE}(remote access)${NC}"
echo ""
echo -e "${YELLOW}📋 Share the bypass URL with visitors - no password required!${NC}"
echo ""
echo -e "${RED}⚠️  Security: No authentication is enabled on the API!${NC}"
echo ""
echo -e "${YELLOW}📋 Logs:${NC}"
echo -e "   Tunnel: /tmp/sol_beast_tunnel_auto.log"
echo -e "   Proxy:  /tmp/sol_beast_proxy.log"
echo ""
echo -e "${RED}Press Ctrl+C to stop all services${NC}"
echo ""

# Show logs
tail -f /tmp/sol_beast_tunnel_auto.log /tmp/sol_beast_proxy.log
