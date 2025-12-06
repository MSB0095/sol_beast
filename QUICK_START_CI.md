# Quick Start: CI/CD from Mobile

📱 **This guide is optimized for mobile users who want to manage their Sol Beast repository entirely from their phone.**

## 🚀 Initial Setup (5 minutes)

### Step 1: Add Repository Secrets

1. Open GitHub app or mobile browser
2. Navigate to your repository
3. Tap **Settings** → **Secrets and variables** → **Actions**
4. Add these three secrets:

| Secret Name | What it is | Where to get it |
|-------------|-----------|-----------------|
| `SOLANA_RPC_URL` | HTTPS RPC endpoint | [Helius.dev](https://helius.dev) (free tier) |
| `SOLANA_WS_URL` | WebSocket endpoint | Same provider as RPC URL |
| `SHYFT_API_KEY` | Shyft GraphQL API key | [Shyft.to](https://shyft.to) (optional but recommended) |

**Example values**:
- `SOLANA_RPC_URL`: `https://mainnet.helius-rpc.com/?api-key=your-key`
- `SOLANA_WS_URL`: `wss://mainnet.helius-rpc.com/?api-key=your-key`
- `SHYFT_API_KEY`: `shyft_api_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx`

### Step 2: Test Your Setup

1. Go to **Actions** tab in your repository
2. Find "Comprehensive CI Pipeline" workflow
3. Tap **Run workflow** button
4. Select your branch
5. Tap **Run workflow**

⏱️ Wait ~10-15 minutes for completion

## 📊 Checking Results

### Viewing Workflow Status

1. **Actions** tab shows all workflow runs
2. **Green checkmark** ✅ = All tests passed
3. **Red X** ❌ = Something failed
4. Tap on any run to see details

### Downloading Test Results

1. Scroll to bottom of workflow run page
2. Find **Artifacts** section
3. Tap artifact name to download:
   - `playwright-ui-screenshots` - Visual state of app
   - `bot-test-results` - Bot monitoring logs
   - `test-summary` - Overall results

All files can be viewed directly on your phone!

## 🔄 Daily Workflow

### When You Push Code

```
You commit → CI runs automatically → Results in ~10 mins
```

Every push to `master`, `main`, or `develop` triggers:
- ✅ Rust unit tests
- ✅ WASM build
- ✅ Frontend build
- ✅ Playwright UI tests
- ✅ Bot functionality tests

### When You Create a PR

```
Open PR → CI runs on PR branch → Review results → Merge
```

CI runs automatically on PRs to ensure changes don't break anything.

## 🎯 Available Workflows

### 1. Comprehensive CI Pipeline (ci.yml)
**Runs**: Automatically on push/PR  
**Purpose**: Complete testing suite  
**Duration**: ~10-15 minutes  
**Artifacts**: Screenshots, logs, test reports

### 2. Test Deployment (test-deployment.yml)
**Runs**: Manual trigger only  
**Purpose**: Validate deployment configuration  
**Duration**: ~8-12 minutes  
**Use when**: Testing major changes

### 3. Deploy to GitHub Pages (deploy.yml)
**Runs**: Automatically on master branch  
**Purpose**: Deploy production app  
**Duration**: ~5-10 minutes  
**Result**: Live app on GitHub Pages

## 🔍 Troubleshooting

### Quick Fixes
- **"Secret not found"**: Check exact spelling - `SOLANA_RPC_URL`, `SOLANA_WS_URL`, `SHYFT_API_KEY`
- **Connection errors**: Verify RPC/WS URLs are correct and use proper protocol (https://, wss://)
- **Rate limiting**: Free tier has limits - check provider dashboard
- **Missing artifacts**: Only kept 7-14 days - re-run if expired

### Detailed Help
📖 **[Complete Troubleshooting Guide](./TROUBLESHOOTING_CI.md)** - Comprehensive solutions for all issues

### Validate Your Setup
Run the **"Validate CI Setup"** workflow to check configuration:
1. Actions tab → "Validate CI Setup" → Run workflow
2. Reviews secrets and tests connectivity
3. Quick diagnosis in ~1 minute

## 📚 More Information

**Detailed guides**:
- [GitHub Secrets Setup Guide](./GITHUB_SECRETS_SETUP.md) - Complete secret configuration
- [Workflows README](./.github/workflows/README.md) - Understanding workflows
- [RPC Configuration Guide](./RPC_CONFIGURATION_GUIDE.md) - RPC provider setup

**Getting API keys**:
- [Helius](https://helius.dev/) - Best for Solana, free 100k req/month
- [QuickNode](https://www.quicknode.com/) - Reliable with good support
- [Shyft](https://shyft.to/) - Enhanced monitoring, free tier available

## ✨ Benefits

✅ **No local machine needed** - Everything runs in GitHub  
✅ **Mobile-friendly** - Manage from your phone  
✅ **Automated testing** - Catches bugs early  
✅ **Visible results** - Screenshots and logs you can review  
✅ **Free** - GitHub Actions free tier is generous  

## 💡 Pro Tips

1. **Star important workflow runs** - Makes them easy to find later
2. **Download artifacts immediately** - They expire after 7-14 days
3. **Use workflow dispatch** - Manually trigger tests when needed
4. **Check notifications** - Enable mobile notifications for CI failures
5. **Review before merge** - Always check CI results before merging PRs

## 🎉 Success Criteria

You're all set when:
- ✅ All three secrets are configured
- ✅ CI workflow runs successfully
- ✅ You can download and view artifacts on your phone
- ✅ Deployments work without errors

**Estimated total setup time**: 5-10 minutes  
**Benefit**: Fully automated development environment from mobile!
