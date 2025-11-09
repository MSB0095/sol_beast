#!/bin/bash

# Sol Beast - Customizable Startup Script
# Choose your preferred terminal emulator and launch method

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Configuration file location
CONFIG_FILE="$HOME/.sol_beast_config"

# Function to save preferences
save_config() {
    cat > "$CONFIG_FILE" << EOF
# Sol Beast User Configuration
TERMINAL_CHOICE="$TERMINAL_CHOICE"
EOF
    echo -e "${GREEN}✓ Configuration saved to $CONFIG_FILE${NC}"
}

# Function to load preferences
load_config() {
    if [ -f "$CONFIG_FILE" ]; then
        source "$CONFIG_FILE"
    fi
}

# Function to show terminal options
show_terminal_menu() {
    echo ""
    echo -e "${BLUE}Available Terminal Emulators:${NC}"
    echo ""
    
    local count=1
    local terminals=()
    
    # Check available terminals
    if command -v konsole &> /dev/null; then
        echo "$count) konsole (KDE Plasma)"
        terminals[$count]="konsole"
        ((count++))
    fi
    
    if command -v gnome-terminal &> /dev/null; then
        echo "$count) gnome-terminal (GNOME)"
        terminals[$count]="gnome-terminal"
        ((count++))
    fi
    
    if command -v xfce4-terminal &> /dev/null; then
        echo "$count) xfce4-terminal (XFCE)"
        terminals[$count]="xfce4-terminal"
        ((count++))
    fi
    
    if command -v tilix &> /dev/null; then
        echo "$count) tilix (Modern terminal)"
        terminals[$count]="tilix"
        ((count++))
    fi
    
    if command -v xterm &> /dev/null; then
        echo "$count) xterm (Classic X terminal)"
        terminals[$count]="xterm"
        ((count++))
    fi
    
    if command -v urxvt &> /dev/null; then
        echo "$count) urxvt (rxvt-unicode)"
        terminals[$count]="urxvt"
        ((count++))
    fi
    
    echo ""
    echo -n "Enter your choice (1-$((count-1))): "
    read -r choice
    
    if [ -n "${terminals[$choice]}" ]; then
        TERMINAL_CHOICE="${terminals[$choice]}"
        save_config
        echo -e "${GREEN}✓ Terminal set to: $TERMINAL_CHOICE${NC}"
    else
        echo -e "${YELLOW}Invalid choice. Using konsole as default.${NC}"
        TERMINAL_CHOICE="konsole"
        save_config
    fi
}

# Function to start in selected terminal
start_in_terminal() {
    local title=$1
    local command=$2
    
    case "$TERMINAL_CHOICE" in
        konsole)
            konsole --title "$title" -e bash -c "cd '$SCRIPT_DIR' && $command; bash" &
            ;;
        gnome-terminal)
            gnome-terminal --title "$title" -- bash -c "cd '$SCRIPT_DIR' && $command; bash"
            ;;
        xfce4-terminal)
            xfce4-terminal --title "$title" -e "cd '$SCRIPT_DIR' && $command" &
            ;;
        tilix)
            tilix --title "$title" -e "cd '$SCRIPT_DIR' && $command" &
            ;;
        xterm)
            xterm -title "$title" -e "cd '$SCRIPT_DIR' && $command" &
            ;;
        urxvt)
            urxvt -title "$title" -e bash -c "cd '$SCRIPT_DIR' && $command; bash" &
            ;;
        *)
            # Fallback to auto-detect
            if command -v konsole &> /dev/null; then
                konsole --title "$title" -e bash -c "cd '$SCRIPT_DIR' && $command; bash" &
            elif command -v gnome-terminal &> /dev/null; then
                gnome-terminal -- bash -c "cd '$SCRIPT_DIR' && $command; bash"
            else
                echo "Terminal not found. Please run manually:"
                echo "$command"
                return 1
            fi
            ;;
    esac
}

# Main menu
show_main_menu() {
    echo ""
    echo -e "${BLUE}╔════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║   Sol Beast - Smart Startup        ║${NC}"
    echo -e "${BLUE}╚════════════════════════════════════╝${NC}"
    echo ""
    echo "Options:"
    echo "1) Start with current settings ($([ -n "$TERMINAL_CHOICE" ] && echo "$TERMINAL_CHOICE" || echo "auto-detect"))"
    echo "2) Change terminal emulator"
    echo "3) View current configuration"
    echo "4) Reset to defaults"
    echo "5) Exit"
    echo ""
    echo -n "Choose an option (1-5): "
    read -r option
    
    case $option in
        1)
            start_services
            ;;
        2)
            show_terminal_menu
            show_main_menu
            ;;
        3)
            echo ""
            echo -e "${BLUE}Current Configuration:${NC}"
            if [ -f "$CONFIG_FILE" ]; then
                cat "$CONFIG_FILE"
            else
                echo "No configuration saved yet. Using defaults."
            fi
            show_main_menu
            ;;
        4)
            rm -f "$CONFIG_FILE"
            echo -e "${GREEN}✓ Configuration reset to defaults${NC}"
            show_main_menu
            ;;
        5)
            echo "Goodbye!"
            exit 0
            ;;
        *)
            echo "Invalid option"
            show_main_menu
            ;;
    esac
}

# Function to start services
start_services() {
    echo ""
    echo -e "${BLUE}Setting up Sol Beast...${NC}"
    
    cd "$SCRIPT_DIR"
    
    echo "Running deployment setup..."
    ./deploy.sh setup
    
    echo ""
    echo -e "${GREEN}Setup complete!${NC}"
    echo ""
    echo -e "${BLUE}Starting services in $TERMINAL_CHOICE terminals...${NC}"
    echo ""
    
    # Start backend
    sleep 1
    start_in_terminal "Sol Beast Backend" "./run-backend.sh"
    echo -e "${GREEN}✓ Backend starting in new terminal${NC}"
    
    # Start frontend
    sleep 2
    start_in_terminal "Sol Beast Frontend" "./run-frontend.sh"
    echo -e "${GREEN}✓ Frontend starting in new terminal${NC}"
    
    echo ""
    echo -e "${BLUE}================================${NC}"
    echo -e "${BLUE}🎉 Sol Beast is Starting!${NC}"
    echo -e "${BLUE}================================${NC}"
    echo ""
    echo -e "${GREEN}Frontend: http://localhost:3000${NC}"
    echo -e "${GREEN}Backend:  http://localhost:8080${NC}"
    echo ""
    echo "If browser doesn't open automatically, visit:"
    echo "http://localhost:3000"
    echo ""
    echo "Press Ctrl+C in each terminal to stop servers"
    echo ""
}

# Main execution
load_config

# If no terminal choice yet, show menu
if [ -z "$TERMINAL_CHOICE" ]; then
    show_terminal_menu
fi

# Check if running interactively or just starting services
if [ $# -eq 0 ]; then
    # Interactive mode
    show_main_menu
else
    # Direct start mode
    start_services
fi
