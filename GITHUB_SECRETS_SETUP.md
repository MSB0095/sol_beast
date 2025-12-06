# GitHub Secrets Setup Guide

This guide explains how to configure repository secrets for automated testing and deployment workflows.

## Overview

The Sol Beast repository uses GitHub Actions for continuous integration and deployment. To run tests that require network access (RPC endpoints, WebSocket connections, API keys), you need to configure repository secrets.

## Required Secrets

### 1. SOLANA_RPC_URL
**Purpose**: HTTP RPC endpoint for Solana blockchain interactions  
**Format**: HTTPS URL  
**Example**: `https://api.mainnet-beta.solana.com`  
**Recommended Providers**:
- [Helius](https://helius.dev/) - Recommended for production
- [QuickNode](https://www.quicknode.com/) - Reliable with good CORS support
- [Alchemy](https://www.alchemy.com/) - Enterprise-grade infrastructure
- Public RPC: `https://api.mainnet-beta.solana.com` (rate-limited, **no CORS for browser**, testing only)

**Used in**:
- CI workflow for bot functionality tests
- Deployment testing workflow
- WASM bot tests

### 2. SOLANA_WS_URL
**Purpose**: WebSocket endpoint for real-time blockchain monitoring  
**Format**: WSS URL  
**Example**: `wss://api.mainnet-beta.solana.com`  
**Recommended Providers**: Same as SOLANA_RPC_URL (most providers give matching WS URLs)

**Important Notes**:
- Must use WSS (secure WebSocket) protocol
- Public Solana RPCs do NOT support browser CORS - premium providers required for WASM mode
- Should be from the same provider as your RPC URL for consistency

**Used in**:
- CI workflow for bot functionality tests
- Deployment testing workflow
- WASM bot WebSocket monitoring

### 3. SHYFT_API_KEY
**Purpose**: API key for Shyft GraphQL service (alternative/enhanced blockchain data provider)  
**Format**: API key string  
**Example**: `shyft_api_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx`  
**Provider**: [Shyft](https://shyft.to/)

**Important Notes**:
- This is a NEW secret requirement for Shyft GraphQL integration
- Optional but recommended for enhanced monitoring capabilities
- Free tier available at Shyft
- Used for GraphQL-based WebSocket subscriptions as alternative to legacy RPC

**Used in**:
- CI workflow bot-settings.json generation
- Bot functionality tests with Shyft monitoring
- CLI bot with Shyft monitor mode

## How to Add Secrets

### From Web Browser (Desktop or Mobile)

1. **Navigate to repository settings**
   - Go to your repository on GitHub
   - Click "Settings" tab
   - In left sidebar, click "Secrets and variables" → "Actions"

2. **Add each secret**
   - Click "New repository secret" button
   - Enter the secret name (exactly as shown above)
   - Paste the secret value
   - Click "Add secret"

3. **Verify secrets are added**
   - You should see all three secrets listed
   - Secret values are hidden after creation (security feature)

### From GitHub CLI (Optional)

If you have GitHub CLI installed:

```bash
# Set SOLANA_RPC_URL
gh secret set SOLANA_RPC_URL --body "https://your-rpc-url.com"

# Set SOLANA_WS_URL
gh secret set SOLANA_WS_URL --body "wss://your-ws-url.com"

# Set SHYFT_API_KEY
gh secret set SHYFT_API_KEY --body "shyft_api_your_key_here"
```

## Secret Usage in Workflows

### CI Workflow (ci.yml)
The main CI pipeline uses secrets in multiple jobs:

- **UI Tests Job**: Creates `bot-settings.json` with RPC URLs and Shyft key
- **Bot Functionality Tests Job**: Passes secrets as environment variables for realistic testing

Example usage:
```yaml
run: |
  cat > bot-settings.json << EOF
  {
    "solana_rpc_urls": ["${{ secrets.SOLANA_RPC_URL || 'https://api.mainnet-beta.solana.com' }}"],
    "solana_ws_urls": ["${{ secrets.SOLANA_WS_URL || 'wss://api.mainnet-beta.solana.com' }}"],
    "shyft_api_key": "${{ secrets.SHYFT_API_KEY || '' }}"
  }
  EOF
```

### Test Deployment Workflow (test-deployment.yml)
Standalone testing workflow for validating deployments:
- Uses all three secrets for comprehensive testing
- Generates bot-settings.json with actual credentials
- Runs Playwright tests against deployed app

### Deploy Workflow (deploy.yml)
Production deployment to GitHub Pages:
- Does NOT use secrets (public deployment)
- Users configure their own RPC URLs in the UI after deployment

## Security Notes

⚠️ **Important Security Considerations**:

1. **Secrets are hidden**: Once added, secret values cannot be viewed in GitHub UI
2. **Not exposed in logs**: GitHub automatically masks secret values in workflow logs
3. **Limited access**: Only workflows in the repository can access these secrets
4. **No client exposure**: Secrets are only used in CI/CD, never shipped to browser
5. **Rotation recommended**: Rotate API keys periodically for security
6. **Free tier limits**: Be aware of rate limits on free RPC/API tiers

## Testing Secret Configuration

After adding secrets, test them by:

1. **Trigger CI workflow**:
   - Push a commit to `master`, `main`, or `develop` branch
   - Or manually trigger workflow from Actions tab

2. **Check workflow results**:
   - Go to "Actions" tab in repository
   - Click on latest workflow run
   - View job logs to verify secrets are being used
   - Download artifacts (screenshots, logs) to inspect results

3. **Mobile-friendly testing**:
   - All artifacts can be downloaded from GitHub mobile app or browser
   - Screenshots show visual state of tests
   - Logs contain detailed test output

## Troubleshooting

### "Secret not found" errors
- Check secret names exactly match (case-sensitive): `SOLANA_RPC_URL`, `SOLANA_WS_URL`, `SHYFT_API_KEY`
- Verify secrets are set at repository level, not organization level
- Ensure you have admin access to the repository

### Connection failures in tests
- Verify RPC/WS URLs are correct and accessible
- Check API key is valid and not expired
- Ensure URLs use correct protocol (https:// for RPC, wss:// for WebSocket)
- Test URLs manually with curl or browser dev tools

### Rate limiting
- Free tier RPC endpoints have strict rate limits
- Consider upgrading to paid tier for CI/CD usage
- Use different endpoints for testing vs production

### CORS errors in browser tests
- Public Solana RPC does NOT support browser CORS
- Use premium provider (Helius, QuickNode, Alchemy) for WASM mode tests
- Backend/CLI mode not affected by CORS

## Getting API Keys

### Helius (Recommended)
1. Visit [https://helius.dev/](https://helius.dev/)
2. Sign up for free account
3. Create new project
4. Copy RPC URL and WS URL
5. Free tier: 100,000 requests/month

### QuickNode
1. Visit [https://www.quicknode.com/](https://www.quicknode.com/)
2. Sign up and create Solana endpoint
3. Copy HTTP and WSS URLs
4. Free trial available

### Shyft
1. Visit [https://shyft.to/](https://shyft.to/)
2. Sign up for account
3. Get API key from dashboard
4. Free tier includes GraphQL access
5. Use for enhanced monitoring features

## Summary

**Minimum required secrets for CI/CD**:
- `SOLANA_RPC_URL` - Required
- `SOLANA_WS_URL` - Required
- `SHYFT_API_KEY` - Optional but recommended

**Time to set up**: ~5 minutes  
**Cost**: Free tier available for all services  
**Benefit**: Fully automated testing environment accessible from mobile device
