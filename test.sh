#!/bin/bash

# Test script to verify Sol Beast is working

echo "🚀 Testing Sol Beast Deployment"
echo ""

# Give it a moment
sleep 2

# Test backend health
echo "Testing backend health..."
HEALTH=$(curl -s http://localhost:8080/api/health 2>&1)

if [[ $HEALTH == *"status"* ]]; then
    echo "✅ Backend is responding!"
    echo "   Response: $HEALTH"
else
    echo "❌ Backend not responding"
    echo "   Response: $HEALTH"
fi

echo ""
echo "Testing frontend..."
FRONTEND=$(curl -s http://localhost:3000 2>&1 | head -1)

if [[ $FRONTEND == *"<!DOCTYPE"* ]] || [[ $FRONTEND == *"html"* ]]; then
    echo "✅ Frontend is responding!"
else
    echo "⚠️  Frontend response: $(echo $FRONTEND | head -c 50)..."
fi

echo ""
echo "Testing API endpoints..."
echo ""

echo "1. Health check:"
curl -s http://localhost:8080/api/health | head -c 100
echo ""
echo ""

echo "2. Stats:"
curl -s http://localhost:8080/api/stats | head -c 100
echo ""
echo ""

echo "3. Settings:"
curl -s http://localhost:8080/api/settings | head -c 100
echo ""
echo ""

echo "✅ All tests complete!"
echo ""
echo "Frontend:  http://localhost:3000"
echo "Backend:   http://localhost:8080"
echo "API:       http://localhost:8080/api"
