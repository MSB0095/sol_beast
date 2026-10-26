#!/bin/bash
# Sol Beast - Cross-Platform Startup Script
# Automatically detects OS and uses native terminal behavior
# No configuration needed - just run: ./start.sh

set -e

# Colors (ANSI)
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Detect OS
detect_os() {
    case "$(uname -s)" in
        Darwin*)  echo "macos" ;;
        Linux*)   echo "linux" ;;
        MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
        *) echo "unknown" ;;
    esac
}

OS=$(detect_os)

# Start backend in a terminal
start_backend() {
    local backend_cmd="cd '$SCRIPT_DIR' && bash ./tools/backend.sh"
    
    case "$OS" in
        macos)
            # macOS: use Terminal.app
            osascript -e "tell app \"Terminal\" to do script \"$backend_cmd\"" 2>/dev/null || {
                echo -e "${YELLOW}⚠ Failed to open Terminal.app, running in background...${NC}"
                eval "$backend_cmd" &
            }
            ;;
        linux)
            # Linux: try available terminals, fallback to background
            if command -v gnome-terminal &>/dev/null; then
                gnome-terminal -- bash -c "$backend_cmd; bash" 2>/dev/null || eval "$backend_cmd" &
            elif command -v xterm &>/dev/null; then
                xterm -title "Sol Beast Backend" -e bash -c "$backend_cmd; bash" 2>/dev/null || eval "$backend_cmd" &
            elif command -v konsole &>/dev/null; then
                konsole --title "Sol Beast Backend" -e bash -c "$backend_cmd; bash" 2>/dev/null || eval "$backend_cmd" &
            else
                eval "$backend_cmd" &
            fi
            ;;
        windows)
            # Windows: use cmd.exe
            cmd.exe /c "cd /d $SCRIPT_DIR && bash ./tools/backend.sh" 2>/dev/null || eval "$backend_cmd" &
            ;;
        *)
            eval "$backend_cmd" &
            ;;
    esac
}

# Start frontend in a terminal
start_frontend() {
    local frontend_cmd="cd '$SCRIPT_DIR' && bash ./tools/frontend.sh"
    
    case "$OS" in
        macos)
            # macOS: use Terminal.app
            osascript -e "tell app \"Terminal\" to do script \"$frontend_cmd\"" 2>/dev/null || {
                echo -e "${YELLOW}⚠ Failed to open Terminal.app, running in background...${NC}"
                eval "$frontend_cmd" &
            }
            ;;
        linux)
            # Linux: try available terminals, fallback to background
            if command -v gnome-terminal &>/dev/null; then
                gnome-terminal -- bash -c "$frontend_cmd; bash" 2>/dev/null || eval "$frontend_cmd" &
            elif command -v xterm &>/dev/null; then
                xterm -title "Sol Beast Frontend" -e bash -c "$frontend_cmd; bash" 2>/dev/null || eval "$frontend_cmd" &
            elif command -v konsole &>/dev/null; then
                konsole --title "Sol Beast Frontend" -e bash -c "$frontend_cmd; bash" 2>/dev/null || eval "$frontend_cmd" &
            else
                eval "$frontend_cmd" &
            fi
            ;;
        windows)
            # Windows: use cmd.exe
            cmd.exe /c "cd /d $SCRIPT_DIR && bash ./tools/frontend.sh" 2>/dev/null || eval "$frontend_cmd" &
            ;;
        *)
            eval "$frontend_cmd" &
            ;;
    esac
}

# Main startup
main() {
    echo ""
    echo -e "${BLUE}╔════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║   Sol Beast - Starting Up          ║${NC}"
    echo -e "${BLUE}╚════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${BLUE}Detected OS:${NC} $OS"
    echo ""
    
    # Run deployment setup
    echo -e "${BLUE}Running setup...${NC}"
    if [ -f "./deploy.sh" ]; then
        bash ./deploy.sh setup 2>/dev/null || true
    fi
    echo -e "${GREEN}✓ Setup complete${NC}"
    echo ""
    
    # Start services
    echo -e "${BLUE}Starting backend...${NC}"
    start_backend
    
    # Wait for the backend to actually be listening before starting the frontend.
    # Release builds can take 60+ seconds; without this the frontend proxy floods
    # the console with ECONNREFUSED errors.
    echo -e "${YELLOW}⏳ Waiting for backend to be ready on port 8081 (compiling release build)...${NC}"
    local waited=0
    local max_wait=300  # 5 minutes max
    while [ $waited -lt $max_wait ]; do
        if curl -s --max-time 2 http://127.0.0.1:8081/healthz >/dev/null 2>&1; then
            break
        fi
        sleep 3
        waited=$((waited + 3))
        # Show a progress dot every 15 seconds
        if [ $((waited % 15)) -eq 0 ]; then
            echo -e "${YELLOW}   ...still compiling (${waited}s elapsed)${NC}"
        fi
    done
    
    if [ $waited -ge $max_wait ]; then
        echo -e "${RED}✗ Backend failed to start within ${max_wait}s. Check logs.${NC}"
        exit 1
    fi
    echo -e "${GREEN}✓ Backend started (took ~${waited}s)${NC}"
    echo ""
    
    echo -e "${BLUE}Starting frontend...${NC}"
    start_frontend
    sleep 2
    echo -e "${GREEN}✓ Frontend started${NC}"
    echo ""
    
    # Display info
    echo -e "${BLUE}════════════════════════════════════${NC}"
    echo -e "${GREEN}🎉 Sol Beast is running!${NC}"
    echo -e "${BLUE}════════════════════════════════════${NC}"
    echo ""
    echo -e "${GREEN}Frontend:  http://localhost:3000${NC}"
    echo -e "${GREEN}Backend:   http://127.0.0.1:8081${NC}"
    echo -e "${GREEN}API:       http://127.0.0.1:8081/v1${NC}"
    echo ""
    
    if [ "$OS" = "macos" ] || [ "$OS" = "windows" ]; then
        echo "✓ Services opened in separate terminal windows"
    else
        echo "✓ Services running in background"
        echo ""
        echo "To stop services:"
        echo "  - Kill the terminal windows, OR"
        echo "  - Press Ctrl+C in each terminal, OR"
        echo "  - Run: pkill -f 'cargo run|npm run dev'"
    fi
    
    echo ""
}

main
