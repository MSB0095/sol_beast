#!/bin/bash

# Sol Beast - Quick Start Script
# Does everything automatically in development mode

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}"
echo "╔════════════════════════════════════╗"
echo "║   Sol Beast - Quick Start          ║"
echo "║   Full Stack Testing Setup         ║"
echo "╚════════════════════════════════════╝"
echo -e "${NC}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Function to start in new terminal
start_in_terminal() {
    local title=$1
    local command=$2
    
    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS
        osascript -e "tell app \"Terminal\" to do script \"cd '$SCRIPT_DIR' && $command\""
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        # Linux - try common terminal emulators (konsole first as it's often better)
        if command -v konsole &> /dev/null; then
            konsole --title "$title" -e bash -c "cd '$SCRIPT_DIR' && $command; bash" &
        elif command -v gnome-terminal &> /dev/null; then
            gnome-terminal -- bash -c "cd '$SCRIPT_DIR' && $command; bash"
        elif command -v xfce4-terminal &> /dev/null; then
            xfce4-terminal --title "$title" -e "cd '$SCRIPT_DIR' && $command" &
        elif command -v xterm &> /dev/null; then
            xterm -title "$title" -e "cd '$SCRIPT_DIR' && $command" &
        else
            echo "No suitable terminal emulator found. Run these commands manually:"
            echo "Terminal 1: $command"
            return 1
        fi
    fi
}

echo "⚙️  Running setup..."
./deploy.sh setup

echo ""
echo -e "${GREEN}Setup complete!${NC}"
echo ""
echo "Starting servers..."
echo ""

# Start backend in new terminal
if start_in_terminal "Sol Beast Backend" "./run-backend.sh"; then
    echo -e "${GREEN}✓ Backend starting in new terminal${NC}"
    sleep 3
else
    echo "Please run in Terminal 1: ./run-backend.sh"
fi

# Start frontend in new terminal
if start_in_terminal "Sol Beast Frontend" "./run-frontend.sh"; then
    echo -e "${GREEN}✓ Frontend starting in new terminal${NC}"
else
    echo "Please run in Terminal 2: ./run-frontend.sh"
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
echo "The frontend should open automatically in your browser."
echo "If not, visit: http://localhost:3000"
echo ""
echo "Press Ctrl+C in each terminal to stop servers"
echo ""
