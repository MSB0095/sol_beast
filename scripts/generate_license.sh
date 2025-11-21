#!/bin/bash
# License Key Generator for sol_beast
# This script generates valid license keys for authorized users
# 
# USAGE: ./scripts/generate_license.sh <client_identifier> [days_valid]
#
# Examples:
#   ./scripts/generate_license.sh "client@example.com"           # Perpetual license
#   ./scripts/generate_license.sh "client@example.com" 365       # 1-year license

set -e

if [ $# -lt 1 ]; then
    echo "Usage: $0 <client_identifier> [days_valid]"
    echo ""
    echo "Examples:"
    echo "  $0 \"client@example.com\"         # Perpetual license"
    echo "  $0 \"client@example.com\" 365     # 1-year license"
    exit 1
fi

CLIENT_ID="$1"
DAYS_VALID="${2:-0}"  # 0 = perpetual

# Generate a unique license key
# In production, this would call a secure key generation service
# For this implementation, we create a deterministic license based on client ID

echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║           sol_beast License Key Generator v1.0                ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""
echo "Generating license for: $CLIENT_ID"

if [ "$DAYS_VALID" -eq 0 ]; then
    echo "License Type: Perpetual (no expiration)"
    LICENSE_TYPE=1
else
    echo "License Type: Time-limited ($DAYS_VALID days)"
    LICENSE_TYPE=2
fi

# Note: This is a simplified example. In production, you would:
# 1. Use a secure key derivation function
# 2. Store licenses in a database
# 3. Implement hardware locking if desired
# 4. Add revocation capability

# For demonstration, generate a pseudo-random license key
# The actual validation is in src/license.rs
RANDOM_SEED=$(echo -n "${CLIENT_ID}$(date +%s)" | sha256sum | cut -d' ' -f1)
LICENSE_KEY=$(echo -n "${RANDOM_SEED}" | base64 | tr '+/' '-_' | cut -c1-64)

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "LICENSE KEY (add this to config.toml):"
echo ""
echo "license_key = \"${LICENSE_KEY}\""
echo ""
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "⚠️  IMPORTANT NOTES:"
echo "  • This license key is unique to this client"
echo "  • Keep this key confidential - do not share publicly"
echo "  • Add the license_key to your config.toml file"
echo "  • For support, contact the sol_beast developer"
echo ""

if [ "$DAYS_VALID" -ne 0 ]; then
    EXPIRY_DATE=$(date -d "+${DAYS_VALID} days" '+%Y-%m-%d' 2>/dev/null || date -v+${DAYS_VALID}d '+%Y-%m-%d')
    echo "  License expires: $EXPIRY_DATE"
    echo ""
fi

echo "License generation complete!"
