# CI/CD Troubleshooting Guide

This guide helps you diagnose and fix common issues with the CI/CD setup.

## 🔍 Quick Diagnosis

### Step 1: Validate Setup
Run the **"Validate CI Setup"** workflow:
1. Go to **Actions** tab
2. Select **"Validate CI Setup"**
3. Click **Run workflow**
4. Review the output

This checks:
- ✅ All required secrets are configured
- ✅ Secret values have correct format
- ✅ RPC endpoint is accessible

---

## Common Issues & Solutions

### Issue: "Secret not found" Error

**Symptom**: Workflow fails with message about missing secret

**Cause**: Secret not configured or misspelled

**Solution**:
1. Go to Settings → Secrets and variables → Actions
2. Verify these exact names (case-sensitive):
   - `SOLANA_RPC_URL`
   - `SOLANA_WS_URL`
   - `SHYFT_API_KEY`
3. Add any missing secrets
4. Re-run the workflow

**Note**: Secret names must match exactly - GitHub Actions is case-sensitive!

---

### Issue: RPC Connection Failures

**Symptom**: Tests fail with connection timeout or "Cannot connect to RPC"

**Possible Causes**:
1. Invalid RPC URL
2. Rate limiting
3. RPC provider outage
4. Incorrect protocol (http vs https)

**Solutions**:

**Check URL format**:
- RPC URL must start with `https://` (not `http://`)
- WebSocket URL must start with `wss://` (not `ws://`)

**Test URL manually**:
```bash
# Test RPC endpoint
curl -X POST YOUR_RPC_URL \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}'

# Should return: {"jsonrpc":"2.0","result":"ok","id":1}
```

**Check rate limits**:
- Free tiers have strict limits
- View provider dashboard for current usage
- Upgrade plan if needed for CI/CD

**Verify provider status**:
- Check [Helius Status](https://status.helius.dev/)
- Check [QuickNode Status](https://status.quicknode.com/)

---

### Issue: CORS Errors in Browser Tests

**Symptom**: Playwright tests fail with "CORS policy" errors

**Cause**: Public Solana RPC doesn't support browser CORS

**Solution**:
1. Use premium RPC provider (Helius, QuickNode, Alchemy)
2. These providers have CORS enabled for browser use
3. Update `SOLANA_RPC_URL` secret with premium URL

**Note**: Backend/CLI mode not affected by CORS - only browser tests

---

### Issue: Workflow Doesn't Start

**Symptom**: No workflow runs appear after push

**Possible Causes**:
1. Pushed to wrong branch
2. Workflow file has syntax error
3. Actions disabled for repository

**Solutions**:

**Check branch**:
- CI runs on: `master`, `main`, `develop` branches
- Check your current branch name
- Push to correct branch or update workflow triggers

**Validate workflow syntax**:
```bash
# In repository root
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"
# Should print nothing if valid
```

**Check Actions are enabled**:
1. Go to repository Settings
2. Click Actions → General
3. Ensure "Allow all actions and reusable workflows" is selected

---

### Issue: Tests Timeout

**Symptom**: Tests run for long time then timeout

**Possible Causes**:
1. Bot tests waiting for blockchain activity
2. Network congestion
3. RPC provider slow response

**Solutions**:

**For bot tests**:
- Normal behavior if no new tokens are launched
- Bot tests monitor for 3 minutes (180 seconds)
- No activity detected is not a failure

**Adjust timeout** (if needed):
Edit `ci.yml` and change duration:
```yaml
# Change from 180 to shorter duration for testing
node test-bot-functionality.mjs http://localhost:8080/sol_beast/ 60
```

**Check RPC latency**:
- Test RPC response time manually
- Consider switching to faster provider

---

### Issue: Artifacts Not Available

**Symptom**: Can't download screenshots or logs

**Possible Causes**:
1. Job didn't complete
2. Artifacts expired (7-14 day retention)
3. Job failed before artifact upload

**Solutions**:

**Check job status**:
- Expand job in workflow run
- Ensure job reached artifact upload step
- Check for errors before upload

**Artifacts expired**:
- Re-run the workflow
- Artifacts kept 7-14 days depending on type
- Download immediately after run completes

**Job failed early**:
- Review job logs to find where it failed
- Fix the issue
- Re-run workflow

---

### Issue: Build Failures

**Symptom**: Rust or frontend build fails

**Possible Causes**:
1. Code compilation error
2. Dependency issues
3. Out of disk space

**Solutions**:

**Check build logs**:
- Click on failed job
- Review error messages
- Look for specific compilation errors

**Dependency cache issues**:
- Sometimes cache gets corrupted
- Workflow automatically handles this with fallback

**Test locally** (if possible):
```bash
# Test Rust build
cargo build --release

# Test WASM build
./build-wasm.sh

# Test frontend build
cd frontend
npm install
npm run build:frontend-webpack
```

---

### Issue: Playwright Tests Fail

**Symptom**: UI tests fail with errors or unexpected behavior

**Possible Causes**:
1. App not loading correctly
2. RPC configuration issues
3. JavaScript errors in app
4. Test script needs update

**Solutions**:

**Download screenshots**:
1. Go to workflow run
2. Download `playwright-ui-screenshots` artifact
3. View PNG files to see visual state

**Check console logs**:
- Review workflow logs
- Look for JavaScript errors
- Common issues: CDN failures (can ignore), API 404s (expected)

**Critical vs non-critical errors**:
- Tests filter out known non-critical errors
- CDN failures: Expected in CI (iconify.design, etc.)
- Backend API 404s: Expected in WASM mode (/health, /settings)
- Focus on actual app errors

---

### Issue: Shyft API Errors

**Symptom**: Tests fail when using Shyft features

**Possible Causes**:
1. Invalid API key
2. API key expired
3. Rate limit exceeded
4. Shyft service outage

**Solutions**:

**Verify API key**:
1. Log in to [Shyft Dashboard](https://shyft.to)
2. Check API key is active
3. Copy fresh API key
4. Update `SHYFT_API_KEY` secret

**Check rate limits**:
- View Shyft dashboard for usage
- Free tier has generous limits
- Upgrade if needed

**Make Shyft optional**:
- Tests work without Shyft key
- Bot falls back to regular RPC monitoring
- Shyft enhances but not required

---

## 📊 Understanding Test Results

### Success Indicators
- ✅ Green checkmarks on all jobs
- ✅ Artifacts generated successfully
- ✅ No critical errors in logs

### Warning Signs (Can Ignore)
- ⚠️ CDN failures (iconify.design, googleapis.com)
- ⚠️ Expected API 404s (/health, /settings)
- ⚠️ Dev resource references (vite.svg, src/main.tsx)

### Real Problems (Need Fixing)
- ❌ Compilation errors
- ❌ RPC connection failures
- ❌ Actual JavaScript exceptions
- ❌ Critical security vulnerabilities

---

## 🔧 Advanced Troubleshooting

### Debugging Workflow Locally

You can test parts of the workflow locally:

```bash
# Build WASM
./build-wasm.sh

# Build frontend
cd frontend
npm ci
npm run build:frontend-webpack

# Run Playwright tests locally
npm install -D playwright serve
npx playwright install chromium
npx serve dist -l 8080 &
SERVER_PID=$!
node ../test-with-playwright.mjs http://localhost:8080/sol_beast/
kill $SERVER_PID
```

### Checking Workflow Syntax

```bash
# Install actionlint (workflow linter)
brew install actionlint  # macOS
# or download from https://github.com/rhysd/actionlint

# Check workflow files
actionlint .github/workflows/*.yml
```

### Reviewing Workflow Logs

1. **Expand all steps**: Click "Show all" to see every step
2. **Search logs**: Use browser Ctrl+F to find specific errors
3. **Download logs**: Click gear icon → "Download log archive"

---

## 📱 Mobile-Specific Issues

### Can't Download Artifacts on Mobile

**Solution**:
- Use GitHub mobile app for better artifact handling
- Or use desktop browser mode on mobile browser
- Artifacts are ZIP files - need app that can extract

### Logs Hard to Read on Small Screen

**Solution**:
- Download logs and view in code editor app
- Use landscape mode for better view
- GitHub Desktop app formats logs better

---

## 🆘 Getting Help

### Before Asking for Help

1. ✅ Run "Validate CI Setup" workflow
2. ✅ Check this troubleshooting guide
3. ✅ Review complete workflow logs
4. ✅ Download and inspect artifacts
5. ✅ Test RPC URLs manually

### Information to Include

When reporting issues, include:
- Workflow run URL
- Complete error message
- Screenshot of failure (if visual)
- Steps you've already tried
- Artifact files (if relevant)

### Documentation Resources

- [Quick Start Guide](./QUICK_START_CI.md)
- [Secrets Setup Guide](./GITHUB_SECRETS_SETUP.md)
- [Workflows README](./.github/workflows/README.md)
- [CI Setup Summary](./CI_SETUP_SUMMARY.md)

---

## ✅ Verification Checklist

Use this to verify your setup is working:

- [ ] All 3 secrets configured (SOLANA_RPC_URL, SOLANA_WS_URL, SHYFT_API_KEY)
- [ ] "Validate CI Setup" workflow runs successfully
- [ ] Can manually trigger "Comprehensive CI Pipeline"
- [ ] All jobs complete without critical errors
- [ ] Artifacts are generated and downloadable
- [ ] Screenshots show app loaded correctly
- [ ] Bot tests show WebSocket connection working
- [ ] Can view all results from mobile device

If all items checked, your CI/CD setup is complete! 🎉

---

## 💡 Pro Tips

1. **Test in small batches**: Don't wait for full 15-min run to find issues
2. **Use validate-setup first**: Quick check before full CI run
3. **Download artifacts immediately**: They expire after 7-14 days
4. **Monitor RPC usage**: Free tiers have limits
5. **Keep secrets rotated**: Update API keys periodically for security
6. **Check provider status pages**: When multiple tests fail suddenly
7. **Use workflow dispatch**: Manually trigger workflows for testing
8. **Review diffs carefully**: Small typos in secrets cause big problems

---

**Still stuck?** Review the workflow logs carefully - they usually contain the answer!
