#!/bin/bash

# Sol Beast - ngrok Tunnel (NO PASSWORD!)
# Better alternative to localtunnel - no password screen for visitors

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

PORT=3000

echo -e "${CYAN}╔══════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║              Sol Beast - ngrok Tunnel (NO PASSWORD!)             ║${NC}"
echo -e "${CYAN}╚══════════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Check if ngrok is installed
if ! command -v ngrok &> /dev/null; then
    echo -e "${RED}❌ ngrok not installed${NC}"
    echo ""
    echo -e "${YELLOW}Install ngrok:${NC}"
    echo -e "${BLUE}1. Visit: https://dashboard.ngrok.com/get-started/setup${NC}"
    echo -e "${BLUE}2. Sign up for free account${NC}"
    echo -e "${BLUE}3. Install ngrok:${NC}"
    echo ""
    echo -e "${CYAN}   # For Linux:${NC}"
    echo -e "   curl -s https://ngrok-agent.s3.amazonaws.com/ngrok.asc | \\"
    echo -e "     sudo tee /etc/apt/trusted.gpg.d/ngrok.asc >/dev/null"
    echo -e "   echo 'deb https://ngrok-agent.s3.amazonaws.com buster main' | \\"
    echo -e "     sudo tee /etc/apt/sources.list.d/ngrok.list"
    echo -e "   sudo apt update && sudo apt install ngrok"
    echo ""
    echo -e "${CYAN}   # Or download directly:${NC}"
    echo -e "   wget https://bin.equinox.io/c/bNyj1mQVY4c/ngrok-v3-stable-linux-amd64.tgz"
    echo -e "   tar xvzf ngrok-v3-stable-linux-amd64.tgz"
    echo -e "   sudo mv ngrok /usr/local/bin/"
    echo ""
    exit 1
fi

# Check if authenticated
if ! ngrok config check &> /dev/null 2>&1; then
    echo -e "${YELLOW}⚠️  ngrok not authenticated${NC}"
    echo ""
    echo -e "${YELLOW}Get your auth token:${NC}"
    echo -e "${BLUE}1. Visit: https://dashboard.ngrok.com/get-started/your-authtoken${NC}"
    echo -e "${BLUE}2. Copy your auth token${NC}"
    echo -e "${BLUE}3. Run: ngrok config add-authtoken YOUR_TOKEN${NC}"
    echo ""
    read -p "Enter your ngrok auth token (or Ctrl+C to exit): " token
    if [ ! -z "$token" ]; then
        ngrok config add-authtoken "$token"
        echo -e "${GREEN}✓ Authentication configured${NC}"
        echo ""
    else
        exit 1
    fi
fi

# Check if port is accessible
echo -e "${BLUE}🔍 Checking if port $PORT is accessible...${NC}"
if ! curl -s http://localhost:$PORT > /dev/null 2>&1; then
    echo -e "${RED}❌ Cannot connect to localhost:$PORT${NC}"
    echo -e "${YELLOW}   Make sure Sol Beast is running first!${NC}"
    echo -e "${BLUE}   Run: ./start.sh cli${NC}"
    echo ""
    exit 1
fi
echo -e "${GREEN}✓ Port $PORT is accessible${NC}"
echo ""

# Start ngrok
echo -e "${GREEN}╔══════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║                  🚀 STARTING NGROK TUNNEL 🚀                     ║${NC}"
echo -e "${GREEN}╚══════════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${CYAN}✨ Features:${NC}"
echo -e "   ✅ NO password screen for visitors!"
echo -e "   ✅ HTTPS automatically enabled"
echo -e "   ✅ Very reliable and fast"
echo -e "   ✅ Web interface at http://localhost:4040"
echo ""
echo -e "${YELLOW}⚠️  Security Warning:${NC}"
echo -e "   Your Sol Beast API has no authentication!"
echo -e "   Only share the URL with trusted users!"
echo ""
echo -e "${BLUE}🌐 Starting tunnel on port $PORT...${NC}"
echo ""
echo -e "${CYAN}Press Ctrl+C to stop${NC}"
echo ""
echo -e "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Start ngrok
ngrok http $PORT --log=stdout
