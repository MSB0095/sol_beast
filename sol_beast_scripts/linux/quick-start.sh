#!/bin/bash

# Sol Beast - Quick Start Script (moved to sol_beast_scripts/linux)
set -e

GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT_DIR"

start_in_terminal() {
    local title=$1
    local command=$2
    if [[ "$OSTYPE" == "darwin"* ]]; then
        osascript -e "tell app \"Terminal\" to do script \"cd '$ROOT_DIR' && $command\""
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        if command -v konsole &> /dev/null; then
            konsole --title "$title" -e bash -c "cd '$ROOT_DIR' && $command; bash" &
        elif command -v gnome-terminal &> /dev/null; then
            gnome-terminal -- bash -c "cd '$ROOT_DIR' && $command; bash"
        elif command -v xfce4-terminal &> /dev/null; then
            xfce4-terminal --title "$title" -e "cd '$ROOT_DIR' && $command" &
        elif command -v xterm &> /dev/null; then
            xterm -title "$title" -e "cd '$ROOT_DIR' && $command" &
        else
            echo "No suitable terminal emulator found. Run these commands manually:"
            echo "Terminal 1: $command"
            return 1
        fi
    fi
}

echo "⚙️  Running setup..."
./sol_beast_scripts/linux/deploy.sh setup
echo ""
echo -e "${GREEN}Setup complete!${NC}"
echo ""
echo "Starting servers..."
echo ""

if start_in_terminal "Sol Beast Backend" "./sol_beast_scripts/linux/start-backend.sh"; then
    echo -e "${GREEN}✓ Backend starting in new terminal${NC}"
    sleep 3
else
    echo "Please run in Terminal 1: ./sol_beast_scripts/linux/start-backend.sh"
fi

if start_in_terminal "Sol Beast Frontend" "./sol_beast_scripts/linux/start-frontend.sh"; then
    echo -e "${GREEN}✓ Frontend starting in new terminal${NC}"
else
    echo "Please run in Terminal 2: ./sol_beast_scripts/linux/start-frontend.sh"
fi

echo ""
echo -e "${BLUE}================================${NC}"
echo -e "${BLUE}🎉 Sol Beast is Starting!${NC}"
echo -e "${BLUE}================================${NC}"
echo ""
echo -e "${GREEN}Frontend: http://localhost:3000${NC}"
echo -e "${GREEN}Backend:  http://localhost:8080${NC}"
echo -e "${GREEN}API:      http://localhost:8080/api${NC}"
echo ""
echo "Press Ctrl+C in each terminal to stop servers"
echo ""
