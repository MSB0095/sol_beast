#!/bin/bash

# Sol Beast Tunnel Script
# Exposes an already-running frontend via localtunnel

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Default values
PORT=3000
COPY_TO_CLIPBOARD=false

# Function to display usage
usage() {
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║               Sol Beast Tunnel - Remote Access                  ║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${YELLOW}Usage:${NC} $0 [options]"
    echo ""
    echo -e "${YELLOW}Options:${NC}"
    echo -e "  ${GREEN}-p, --port PORT${NC}        Port to expose (default: 3000)"
    echo -e "  ${GREEN}-c, --copy${NC}             Copy URL to clipboard"
    echo -e "  ${GREEN}-h, --help${NC}             Show this help message"
    echo ""
    echo -e "${YELLOW}Examples:${NC}"
    echo -e "  $0                           ${BLUE}# Expose port 3000 with random URL${NC}"
    echo -e "  $0 -p 8080                   ${BLUE}# Expose custom port${NC}"
    echo -e "  $0 -c                        ${BLUE}# Copy URL to clipboard${NC}"
    echo ""
    echo -e "${CYAN}Note:${NC} Using random URLs to support multiple concurrent users"
    echo ""
    echo -e "${YELLOW}Note:${NC} Make sure your Sol Beast frontend is already running!"
    echo ""
    exit 0
}

# Function to check if command exists
command_exists() {
    command -v "$1" &> /dev/null
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -p|--port)
            PORT="$2"
            shift 2
            ;;
        -c|--copy)
            COPY_TO_CLIPBOARD=true
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo -e "${RED}❌ Unknown option: $1${NC}"
            echo ""
            usage
            ;;
    esac
done

# Check if port is accessible
echo -e "${BLUE}🔍 Checking if port $PORT is accessible...${NC}"
if ! curl -s http://localhost:$PORT > /dev/null 2>&1; then
    echo -e "${RED}❌ Cannot connect to localhost:$PORT${NC}"
    echo -e "${YELLOW}   Make sure Sol Beast is running first!${NC}"
    echo -e "${BLUE}   Run: ./start.sh${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Port $PORT is accessible${NC}"
echo ""

# Check if localtunnel is installed
if ! command_exists lt && ! command_exists npx; then
    echo -e "${RED}❌ localtunnel not found${NC}"
    echo -e "${YELLOW}   Installing localtunnel...${NC}"
    cd "$PROJECT_DIR/frontend"
    npm install localtunnel
    cd "$PROJECT_DIR"
fi

# Function to cleanup on exit
cleanup() {
    echo ""
    echo -e "${YELLOW}🛑 Closing tunnel...${NC}"
    if [ ! -z "$TUNNEL_PID" ]; then
        kill $TUNNEL_PID 2>/dev/null || true
    fi
    echo -e "${GREEN}✓ Tunnel closed${NC}"
    exit 0
}

trap cleanup EXIT INT TERM

# Start the tunnel
echo -e "${BLUE}🌐 Starting localtunnel...${NC}"
echo -e "${YELLOW}⚠️  Security Warning: Your frontend will be publicly accessible!${NC}"
echo -e "${BLUE}    Using random URL to support multiple concurrent users...${NC}"
echo ""

cd "$PROJECT_DIR/frontend"
npx localtunnel --port "$PORT" > /tmp/sol_beast_tunnel_standalone.log 2>&1 &
TUNNEL_PID=$!
cd "$PROJECT_DIR"

# Wait for tunnel to establish and extract URL
echo -e "${YELLOW}⏳ Establishing tunnel connection...${NC}"
sleep 3

TUNNEL_URL=""
for i in {1..15}; do
    if [ -f /tmp/sol_beast_tunnel_standalone.log ]; then
        TUNNEL_URL=$(grep -oP 'https://[^\s]+' /tmp/sol_beast_tunnel_standalone.log | head -1)
        if [ ! -z "$TUNNEL_URL" ]; then
            break
        fi
    fi
    sleep 1
done

if [ -z "$TUNNEL_URL" ]; then
    echo -e "${YELLOW}⚠️  Could not detect tunnel URL automatically${NC}"
    echo -e "${YELLOW}    URL should appear shortly...${NC}"
    echo ""
    echo -e "${BLUE}📋 Check the log file for the tunnel URL:${NC}"
    echo -e "    /tmp/sol_beast_tunnel_standalone.log"
    echo ""
    
    # Try to get any URL from the log
    sleep 5
    TUNNEL_URL=$(grep -oP 'https://[^\s]+' /tmp/sol_beast_tunnel_standalone.log | tail -1)
fi

if [ ! -z "$TUNNEL_URL" ]; then
    echo -e "${GREEN}╔══════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║                  🌍 TUNNEL ACTIVE 🌍                             ║${NC}"
    echo -e "${GREEN}╚══════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${CYAN}🔗 Tunnel URL:${NC}   ${GREEN}${TUNNEL_URL}${NC}"
    echo -e "${CYAN}📍 Local Port:${NC}   $PORT"
    echo ""
    echo -e "${YELLOW}📋 BYPASS PASSWORD SCREEN:${NC}"
    echo -e "${YELLOW}   localtunnel requires a password by default.${NC}"
    echo -e "${YELLOW}   To skip it, start the bypass proxy in another terminal:${NC}"
    echo ""
    echo -e "${BLUE}   node $PROJECT_DIR/scripts/tunnel-proxy.js ${TUNNEL_URL} 8888${NC}"
    echo ""
    echo -e "${YELLOW}   Then share this URL instead: ${GREEN}http://YOUR_PUBLIC_IP:8888${NC}"
    echo ""
    
    # Copy to clipboard if requested
    if [ "$COPY_TO_CLIPBOARD" = true ]; then
        if command_exists xclip; then
            echo -n "$TUNNEL_URL" | xclip -selection clipboard
            echo -e "${GREEN}✓ URL copied to clipboard (xclip)${NC}"
        elif command_exists xsel; then
            echo -n "$TUNNEL_URL" | xsel --clipboard
            echo -e "${GREEN}✓ URL copied to clipboard (xsel)${NC}"
        elif command_exists pbcopy; then
            echo -n "$TUNNEL_URL" | pbcopy
            echo -e "${GREEN}✓ URL copied to clipboard (pbcopy)${NC}"
        else
            echo -e "${YELLOW}⚠️  Clipboard tool not found (install xclip or xsel)${NC}"
        fi
        echo ""
    fi
    
    echo -e "${YELLOW}📋 Share this URL with anyone to access your Sol Beast instance${NC}"
    echo ""
    echo -e "${RED}⚠️  Security: No authentication is enabled!${NC}"
    echo -e "${RED}    Anyone with this URL can access your trading dashboard${NC}"
    echo ""
fi

echo -e "${YELLOW}📋 Logs:${NC} /tmp/sol_beast_tunnel_standalone.log"
echo ""
echo -e "${RED}Press Ctrl+C to stop the tunnel${NC}"
echo ""

# Keep the script running and show tunnel logs
tail -f /tmp/sol_beast_tunnel_standalone.log
