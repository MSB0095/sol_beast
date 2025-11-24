#!/bin/bash

# Quick Integration Test Script
# Tests all backend endpoints and verifies frontend can connect

echo "🧪 Sol Beast Integration Test"
echo "================================"
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Base URL
BASE_URL="http://localhost:8080"

# Function to test endpoint
test_endpoint() {
    local endpoint=$1
    local name=$2
    local method=${3:-GET}
    
    echo -n "Testing $name... "
    
    if [ "$method" = "GET" ]; then
        response=$(curl -s -w "\n%{http_code}" "$BASE_URL$endpoint")
    else
        response=$(curl -s -w "\n%{http_code}" -X "$method" "$BASE_URL$endpoint")
    fi
    
    http_code=$(echo "$response" | tail -n1)
    body=$(echo "$response" | head -n-1)
    
    if [ "$http_code" -eq 200 ]; then
        echo -e "${GREEN}✓ OK${NC} (HTTP $http_code)"
        return 0
    else
        echo -e "${RED}✗ FAILED${NC} (HTTP $http_code)"
        echo "   Response: $body"
        return 1
    fi
}

# Check if backend is running
echo "Checking if backend is running on port 8080..."
if ! curl -s "$BASE_URL/api/health" > /dev/null 2>&1; then
    echo -e "${RED}✗ Backend is not running!${NC}"
    echo ""
    echo "Please start the backend first:"
    echo "  ./run-backend.sh"
    echo "  or"
    echo "  cargo run --release"
    exit 1
fi

echo -e "${GREEN}✓ Backend is running${NC}"
echo ""

# Test all endpoints
echo "Testing API Endpoints:"
echo "====================="

passed=0
failed=0

if test_endpoint "/api/health" "Health Check"; then ((passed++)); else ((failed++)); fi
if test_endpoint "/api/stats" "Statistics"; then ((passed++)); else ((failed++)); fi
if test_endpoint "/api/settings" "Settings"; then ((passed++)); else ((failed++)); fi
if test_endpoint "/api/bot/state" "Bot State"; then ((passed++)); else ((failed++)); fi
if test_endpoint "/api/logs" "Logs"; then ((passed++)); else ((failed++)); fi

echo ""
echo "Testing Bot Control:"
echo "===================="

# Get current state first
current_state=$(curl -s "$BASE_URL/api/bot/state" | grep -o '"running_state":"[^"]*"' | cut -d'"' -f4)
echo "Current bot state: $current_state"

# Test mode switch
echo -n "Testing mode switch to real... "
response=$(curl -s -X POST -H "Content-Type: application/json" \
    -d '{"mode":"real"}' "$BASE_URL/api/bot/mode")
if echo "$response" | grep -q "success"; then
    echo -e "${GREEN}✓ OK${NC}"
    ((passed++))
else
    echo -e "${RED}✗ FAILED${NC}"
    ((failed++))
fi

echo -n "Testing mode switch back to dry-run... "
response=$(curl -s -X POST -H "Content-Type: application/json" \
    -d '{"mode":"dry-run"}' "$BASE_URL/api/bot/mode")
if echo "$response" | grep -q "success"; then
    echo -e "${GREEN}✓ OK${NC}"
    ((passed++))
else
    echo -e "${RED}✗ FAILED${NC}"
    ((failed++))
fi

echo ""
echo "Detailed API Responses:"
echo "======================="

echo ""
echo "📊 Statistics:"
curl -s "$BASE_URL/api/stats" | python3 -m json.tool 2>/dev/null || echo "Failed to parse JSON"

echo ""
echo "📝 Logs (last 5):"
curl -s "$BASE_URL/api/logs" | python3 -m json.tool 2>/dev/null | tail -20 || echo "Failed to parse JSON"

echo ""
echo "⚙️  Bot State:"
curl -s "$BASE_URL/api/bot/state" | python3 -m json.tool 2>/dev/null || echo "Failed to parse JSON"

echo ""
echo "================================"
echo "Test Summary:"
echo "================================"
echo -e "Passed: ${GREEN}$passed${NC}"
echo -e "Failed: ${RED}$failed${NC}"
echo ""

if [ $failed -eq 0 ]; then
    echo -e "${GREEN}🎉 All tests passed!${NC}"
    echo ""
    echo "✅ Backend integration complete"
    echo "✅ All endpoints working"
    echo "✅ Bot control functional"
    echo ""
    echo "Next steps:"
    if [ -d "sol_beast_frontend" ]; then
        FRONTEND_DIR="sol_beast_frontend"
    else
        FRONTEND_DIR="frontend"
    fi
    echo "1. Start the frontend: cd $FRONTEND_DIR && npm run dev"
    echo "2. Open browser to http://localhost:5173"
    echo "3. Check the Logs tab for real-time updates"
    exit 0
else
    echo -e "${RED}❌ Some tests failed${NC}"
    echo ""
    echo "Please check the backend logs for errors"
    exit 1
fi
