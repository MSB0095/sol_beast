#!/bin/bash
set -e

echo "Building SolBeast Dev Fee Smart Contract..."
echo "==========================================="

# Check if cargo-build-sbf is installed
if ! command -v cargo-build-sbf &> /dev/null; then
    echo "Error: cargo-build-sbf not found"
    echo "Please install Solana CLI tools:"
    echo "  sh -c \"\$(curl -sSfL https://release.solana.com/stable/install)\""
    exit 1
fi

# Build the contract
echo "Building contract with cargo-build-sbf..."
cargo build-sbf

# Check if build was successful
if [ ! -f "target/deploy/solbeast_dev_fee.so" ]; then
    echo "Error: Build failed, .so file not found"
    exit 1
fi

# Show binary size
echo ""
echo "Build successful!"
echo "=================="
FILESIZE=$(stat -f%z "target/deploy/solbeast_dev_fee.so" 2>/dev/null || stat -c%s "target/deploy/solbeast_dev_fee.so" 2>/dev/null)
echo "Binary size: $FILESIZE bytes"

if [ "$FILESIZE" -gt 500 ]; then
    echo "Warning: Binary size exceeds 500 bytes target!"
else
    echo "✓ Binary size is within 500 byte target"
fi

echo ""
echo "Binary location: target/deploy/solbeast_dev_fee.so"
echo ""
echo "To deploy to devnet:"
echo "  solana config set --url devnet"
echo "  solana program deploy target/deploy/solbeast_dev_fee.so"
echo ""
echo "To deploy to mainnet:"
echo "  solana config set --url mainnet-beta"
echo "  solana program deploy target/deploy/solbeast_dev_fee.so"
