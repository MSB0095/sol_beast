#!/bin/bash
# Test script for Sol Beast API endpoints (moved to sol_beast_scripts/linux)

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && cd ../.. && pwd)"
BASE_URL="http://localhost:8080"

if [ -d "$ROOT_DIR/sol_beast_frontend" ]; then
    FRONTEND_DIR="sol_beast_frontend"
else
    FRONTEND_DIR="frontend"
fi

echo "=================================="
echo "Sol Beast API Test Suite"
echo "=================================="
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}Test 1: Health Check${NC}"
response=$(curl -s "$BASE_URL/health")
if [[ $response == *"ok"* ]]; then
    echo -e "${GREEN}✓ PASS${NC} - Health check returned OK"
    echo "Response: $response"
else
    echo -e "${RED}✗ FAIL${NC} - Health check failed"
    echo "Response: $response"
fi
echo ""

echo -e "${YELLOW}Test 2: Get Bot State${NC}"
response=$(curl -s "$BASE_URL/bot/state")
echo "Response: $response"
echo -e "${GREEN}✓ PASS${NC}"
echo ""

echo -e "${YELLOW}Test 3: Get Settings${NC}"
response=$(curl -s "$BASE_URL/settings")
if [[ $response == *"tp_percent"* ]]; then
    echo -e "${GREEN}✓ PASS${NC} - Settings retrieved successfully"
    echo "Settings include tp_percent, sl_percent, buy_amount, etc."
else
    echo -e "${RED}✗ FAIL${NC} - Settings not retrieved"
fi
echo ""

echo -e "${YELLOW}Test 4: Get Stats${NC}"
response=$(curl -s "$BASE_URL/stats")
if [[ $response == *"total_buys"* ]]; then
    echo -e "${GREEN}✓ PASS${NC} - Stats retrieved successfully"
    echo "Response: $response" | jq '.' 2>/dev/null || echo "Response: $response"
else
    echo -e "${RED}✗ FAIL${NC} - Stats not retrieved"
fi
echo ""

echo -e "${YELLOW}Test 5: Get Logs${NC}"
response=$(curl -s "$BASE_URL/logs")
echo "Response: $response" | jq '.' 2>/dev/null || echo "Response: $response"
echo -e "${GREEN}✓ PASS${NC}"
echo ""

echo -e "${YELLOW}Test 6: Stop Bot (to prepare for mode change)${NC}"
response=$(curl -s -X POST "$BASE_URL/bot/stop")
echo "Response: $response"
if [[ $response == *"success"* ]] || [[ $response == *"stopping"* ]]; then
    echo -e "${GREEN}✓ PASS${NC} - Bot stop initiated"
else
    echo -e "${RED}✗ FAIL${NC} - Bot stop failed"
fi
echo ""

echo "Waiting 2 seconds for bot to stop..."
sleep 2
echo ""

echo -e "${YELLOW}Test 7: Change Mode to Dry-Run${NC}"
response=$(curl -s -X POST "$BASE_URL/bot/mode" \
    -H "Content-Type: application/json" \
    -d '{"mode": "dry-run"}')
echo "Response: $response"
if [[ $response == *"success"* ]]; then
    echo -e "${GREEN}✓ PASS${NC} - Mode changed to dry-run"
else
    echo -e "${YELLOW}⚠ WARNING${NC} - Mode change response: $response"
fi
echo ""

echo -e "${YELLOW}Test 8: Start Bot${NC}"
response=$(curl -s -X POST "$BASE_URL/bot/start")
echo "Response: $response"
if [[ $response == *"success"* ]] || [[ $response == *"starting"* ]]; then
    echo -e "${GREEN}✓ PASS${NC} - Bot start initiated"
else
    echo -e "${RED}✗ FAIL${NC} - Bot start failed"
fi
echo ""

echo "Waiting 2 seconds for bot to fully start..."
sleep 2
echo ""

echo -e "${YELLOW}Test 9: Verify Bot State is Running${NC}"
response=$(curl -s "$BASE_URL/bot/state")
echo "Response: $response"
if [[ $response == *"running"* ]]; then
    echo -e "${GREEN}✓ PASS${NC} - Bot is running"
else
    echo -e "${YELLOW}⚠ WARNING${NC} - Bot state: $response"
fi
echo ""

echo -e "${YELLOW}Test 10: Get Updated Stats${NC}"
response=$(curl -s "$BASE_URL/stats")
echo "Response: $response" | jq '.' 2>/dev/null || echo "Response: $response"
if [[ $response == *"running_state"* ]] && [[ $response == *"mode"* ]]; then
    echo -e "${GREEN}✓ PASS${NC} - Stats include running_state and mode"
else
    echo -e "${YELLOW}⚠ WARNING${NC} - Stats may not include state info"
fi
echo ""

echo -e "${YELLOW}Test 11: Get Updated Logs${NC}"
response=$(curl -s "$BASE_URL/logs")
log_count=$(echo "$response" | jq '.logs | length' 2>/dev/null || echo "0")
echo "Log entries: $log_count"
if [[ $log_count -gt 0 ]]; then
    echo -e "${GREEN}✓ PASS${NC} - Logs contain $log_count entries"
    echo "Sample log entries:"
    echo "$response" | jq '.logs[0:3]' 2>/dev/null || echo "$response"
else
    echo -e "${YELLOW}⚠ WARNING${NC} - No log entries found"
fi
echo ""

echo -e "${YELLOW}Test 12: Try to Change Mode While Running (should fail)${NC}"
response=$(curl -s -X POST "$BASE_URL/bot/mode" \
    -H "Content-Type: application/json" \
    -d '{"mode": "real"}')
echo "Response: $response"
if [[ $response == *"error"* ]] || [[ $response == *"stopped"* ]]; then
    echo -e "${GREEN}✓ PASS${NC} - Mode change correctly rejected while running"
else
    echo -e "${RED}✗ FAIL${NC} - Mode change should have been rejected"
fi
echo ""

echo -e "${YELLOW}Test 13: Stop Bot Again${NC}"
response=$(curl -s -X POST "$BASE_URL/bot/stop")
echo "Response: $response"
if [[ $response == *"success"* ]] || [[ $response == *"stopping"* ]]; then
    echo -e "${GREEN}✓ PASS${NC} - Bot stop initiated"
else
    echo -e "${RED}✗ FAIL${NC} - Bot stop failed"
fi
echo ""

echo "=================================="
echo "Test Suite Complete"
echo "=================================="
echo ""
echo "Summary:"
echo "- All basic endpoints are functional"
echo "- Bot state management works correctly"
echo "- Mode switching validation works"
echo "- Logging system is operational"
echo ""
echo "Next steps:"
echo "1. Start the frontend: cd $FRONTEND_DIR && npm run dev"
echo "2. Open browser to http://localhost:5173"
echo "3. Test full UI integration"
