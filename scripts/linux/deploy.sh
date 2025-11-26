#!/bin/bash

# Sol Beast - Complete Deployment & Testing Script
# Moved to sol_beast_scripts/linux/

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
FRONTEND_PORT=3000
BACKEND_PORT=8080
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Root directory (up from sol_beast_scripts/linux -> project root)
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Detect frontend directory: prefer sol_beast_frontend if it exists
if [ -d "$ROOT_DIR/sol_beast_frontend" ]; then
    FRONTEND_DIR="$ROOT_DIR/sol_beast_frontend"
else
    FRONTEND_DIR="$ROOT_DIR/frontend"
fi

print_header() {
    echo -e "${BLUE}================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}================================${NC}"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ $1${NC}"
}

check_requirements() {
    print_header "Checking Requirements"
    
    # Check Node.js
    if ! command -v node &> /dev/null; then
        print_error "Node.js is not installed"
        exit 1
    fi
    print_success "Node.js $(node -v)"
    
    # Check npm
    if ! command -v npm &> /dev/null; then
        print_error "npm is not installed"
        exit 1
    fi
    print_success "npm $(npm -v)"
    
    # Check Rust
    if ! command -v cargo &> /dev/null; then
        print_error "Rust/Cargo is not installed"
        exit 1
    fi
    print_success "Rust $(rustc --version)"
    
    echo ""
}

setup_frontend() {
    print_header "Setting Up Frontend"
    
    if [ ! -d "$FRONTEND_DIR" ]; then
        print_error "${FRONTEND_DIR} directory not found"
        return 1
    fi
    
    cd "$FRONTEND_DIR"
    
    if [ ! -d "node_modules" ]; then
        print_info "Installing npm dependencies..."
        npm install --legacy-peer-deps --ignore-scripts
        print_success "Frontend dependencies installed"
    else
        print_success "Frontend dependencies already installed"
    fi
    
    cd "$ROOT_DIR"
}

setup_backend() {
    print_header "Setting Up Backend"
    
    if [ ! -f "$ROOT_DIR/Cargo.toml" ]; then
        print_error "Cargo.toml not found"
        return 1
    fi
    
    print_info "Checking Rust dependencies..."
    cargo check
    print_success "Backend dependencies verified"
}

build_backend() {
    print_header "Building Backend (Release)"
    
    print_info "This may take 2-5 minutes on first build..."
    cargo build --release
    print_success "Backend built successfully"
}

check_ports() {
    print_header "Checking Ports"
    
    # Check frontend port
    if lsof -Pi :${FRONTEND_PORT} -sTCP:LISTEN -t >/dev/null 2>&1; then
        print_warning "Port ${FRONTEND_PORT} is already in use"
        return 1
    fi
    print_success "Port ${FRONTEND_PORT} is available"
    
    # Check backend port
    if lsof -Pi :${BACKEND_PORT} -sTCP:LISTEN -t >/dev/null 2>&1; then
        print_warning "Port ${BACKEND_PORT} is already in use"
        return 1
    fi
    print_success "Port ${BACKEND_PORT} is available"
    
    echo ""
}

display_startup_info() {
    echo ""
    print_header "Startup Information"
    
    echo -e "${GREEN}Frontend URL:${NC} http://localhost:${FRONTEND_PORT}"
    echo -e "${GREEN}Backend URL:${NC} http://localhost:${BACKEND_PORT}"
    echo -e "${GREEN}API Base:${NC} http://localhost:${BACKEND_PORT}/api"
    echo ""
    echo -e "${YELLOW}Terminal Commands:${NC}"
    echo "  Frontend: npm run dev (in $FRONTEND_DIR)"
    echo "  Backend:  cargo run --release"
    echo ""
}

start_backend() {
    print_header "Starting Backend Server"
    
    print_info "Starting on port ${BACKEND_PORT}..."
    print_info "Press Ctrl+C to stop"
    echo ""
    
    cd "$ROOT_DIR"
    export RUST_LOG=info
    exec cargo run --release
}

start_frontend() {
    print_header "Starting Frontend Server"
    
    print_info "Starting on port ${FRONTEND_PORT}..."
    print_info "Press Ctrl+C to stop"
    echo ""
    
    cd "$FRONTEND_DIR"
    exec npm run dev
}

start_all() {
    print_header "Starting Both Servers"
    
    print_info "Opening two terminals would be ideal, but starting backend first..."
    print_info "After backend starts, run: ./run-frontend.sh in another terminal"
    echo ""
    
    sleep 2
    start_backend
}

show_usage() {
    cat << EOF
${BLUE}Sol Beast - Deployment & Testing Script${NC}

${GREEN}Usage:${NC}
    ./deploy.sh [command]

${GREEN}Commands:${NC}
    setup           Setup both frontend and backend
    build           Build backend for production
    check           Check requirements and ports
    start-all       Start both servers (use 2 terminals)
    start-backend   Start only backend server
    start-frontend  Start only frontend server (in another terminal)
    clean           Clean build artifacts
    help            Show this help message

${GREEN}Examples:${NC}
    # Full setup and start
    ./deploy.sh setup
    ./deploy.sh start-all

    # Or in separate terminals:
    ./deploy.sh start-backend    # Terminal 1
    ./deploy.sh start-frontend   # Terminal 2

${YELLOW}First Time Setup:${NC}
    1. ./deploy.sh check          # Verify requirements
    2. ./deploy.sh setup          # Install dependencies
    3. Open two terminal windows
    4. Terminal 1: ./deploy.sh start-backend
    5. Terminal 2: ./deploy.sh start-frontend
    6. Visit http://localhost:3000

${BLUE}Documentation:${NC}
    - QUICK_REFERENCE.md - Quick overview
    - DEPLOYMENT_GUIDE.md - Detailed deployment
    - FRONTEND_GUIDE.md - Frontend setup
    - INTEGRATION_EXAMPLE.md - Backend integration

EOF
}

# Main logic
case "${1:-help}" in
    setup)
        check_requirements
        setup_frontend
        setup_backend
        print_success "Setup complete!"
        display_startup_info
        ;;
    build)
        check_requirements
        build_backend
        print_success "Build complete!"
        ;;
    check)
        check_requirements
        check_ports
        print_success "All checks passed!"
        ;;
    start-all)
        check_requirements
        check_ports
        start_all
        ;;
    start-backend)
        print_header "Starting Backend"
        start_backend
        ;;
    start-frontend)
        print_header "Starting Frontend"
        start_frontend
        ;;
    clean)
        print_header "Cleaning Build Artifacts"
        cargo clean
        rm -rf "$FRONTEND_DIR/dist" "$FRONTEND_DIR/node_modules"
        print_success "Clean complete"
        ;;
    help|--help|-h)
        show_usage
        ;;
    *)
        print_error "Unknown command: $1"
        show_usage
        exit 1
        ;;
esac
#!/bin/bash

# Wrapper: deploy helper in new scripts folder (Linux/macOS)
set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Execute the original deploy.sh while keeping relative pointers working
bash "$SCRIPT_DIR/../../deploy.sh" "$@"
