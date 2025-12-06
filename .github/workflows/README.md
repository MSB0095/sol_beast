# GitHub Actions Workflows

This directory contains GitHub Actions workflows for continuous integration, deployment, and testing.

## Workflows Overview

### 1. ci.yml - Comprehensive CI Pipeline
**Trigger**: Push to main branches, Pull Requests  
**Purpose**: Complete automated testing and validation  
**Jobs**:
- Build Rust/WASM components with unit tests
- Build frontend with webpack
- Run Playwright UI tests with local server
- Run WASM bot functionality tests
- Generate test summary with all results

**Secrets Used**:
- `SOLANA_RPC_URL` - Required for bot tests
- `SOLANA_WS_URL` - Required for WebSocket monitoring tests
- `SHYFT_API_KEY` - Optional, for enhanced monitoring

**Artifacts Generated**:
- WASM build files
- Frontend build
- Playwright screenshots
- Bot test results and logs
- Test summary report

**Duration**: ~10-15 minutes  
**When to use**: Automatically runs on every push/PR, or trigger manually for testing

---

### 2. deploy.yml - Deploy to GitHub Pages
**Trigger**: Push to master branch, Manual dispatch  
**Purpose**: Build and deploy production version to GitHub Pages  
**Jobs**:
- Build WASM, documentation, and frontend
- Deploy to GitHub Pages environment

**Secrets Used**: None (public deployment)

**Artifacts Generated**: GitHub Pages deployment

**Duration**: ~5-10 minutes  
**When to use**: Automatic on master branch commits, manual trigger for redeployment

---

### 3. test-deployment.yml - Test GitHub Pages Deployment
**Trigger**: Manual workflow dispatch only  
**Purpose**: Validate that the app works correctly before/after deployment  
**Jobs**:
- Build complete app with test configuration
- Run Playwright tests against local server
- Capture screenshots and console output

**Secrets Used**:
- `SOLANA_RPC_URL` - For realistic RPC testing
- `SOLANA_WS_URL` - For WebSocket testing
- `SHYFT_API_KEY` - For Shyft API testing

**Artifacts Generated**:
- Deployment screenshot
- Console logs
- Test results

**Duration**: ~8-12 minutes  
**When to use**: Before merging significant changes, debugging deployment issues

---

## Workflow Dependencies

```
ci.yml (on push/PR)
├── build-rust-wasm → build-frontend → ui-tests
└── build-rust-wasm → build-frontend → bot-functionality-tests

deploy.yml (on push to master)
└── build → deploy

test-deployment.yml (manual only)
└── test-build → playwright-tests
```

## Using Workflows from Mobile

All workflows are designed to be mobile-friendly:

1. **Trigger workflows**:
   - GitHub app → Repository → Actions tab
   - Select workflow → Run workflow button (for manual triggers)

2. **Monitor progress**:
   - Actions tab shows real-time status
   - Click on workflow run to see job details
   - Green checkmark = success, Red X = failure

3. **Download artifacts**:
   - Scroll to bottom of workflow run page
   - Click artifact name to download
   - View screenshots, logs, and reports on your device

4. **Review logs**:
   - Click on any job to expand
   - Click on any step to see detailed logs
   - Secrets are automatically masked in logs

## Setting Up Secrets

Before workflows can fully function, configure repository secrets. See [GITHUB_SECRETS_SETUP.md](../../GITHUB_SECRETS_SETUP.md) for detailed instructions.

**Required secrets**:
- `SOLANA_RPC_URL` - Solana RPC endpoint URL
- `SOLANA_WS_URL` - Solana WebSocket endpoint URL
- `SHYFT_API_KEY` - Shyft API key (optional but recommended)

## Common Workflow Patterns

### Running CI on Feature Branch
```bash
git checkout -b feature/my-feature
# Make changes
git commit -m "Add new feature"
git push origin feature/my-feature
# CI workflow runs automatically
# Create PR to merge → CI runs again
```

### Manual Testing Before Deployment
```
1. Go to Actions tab
2. Select "Test GitHub Pages Deployment"
3. Click "Run workflow"
4. Select branch
5. Click "Run workflow" button
6. Wait for completion (~10 mins)
7. Download artifacts to review results
```

### Deploying to Production
```
# Merge PR to master branch
# deploy.yml runs automatically
# App deployed to GitHub Pages

# OR manually trigger:
1. Go to Actions tab
2. Select "Deploy to GitHub Pages"
3. Click "Run workflow"
4. Click "Run workflow" button
```

## Troubleshooting

### Workflow fails with "secret not found"
- Check secrets are configured in Settings → Secrets and variables → Actions
- Verify secret names match exactly: `SOLANA_RPC_URL`, `SOLANA_WS_URL`, `SHYFT_API_KEY`

### Workflow fails at build step
- Check build logs for specific error
- May be dependency issue or code compilation error
- Run locally: `./build-wasm.sh && cd frontend && npm install && npm run build`

### Playwright tests fail
- Download screenshot artifact to see visual state
- Review console logs in workflow output
- Common issues: RPC rate limiting, network timeouts, CORS errors

### Bot tests timeout or fail
- Verify RPC/WS URLs in secrets are correct and accessible
- Check rate limits on RPC provider
- May need to wait for blockchain activity (new token launches)

### Artifacts not generated
- Check job completed (even with failure)
- Artifacts only kept for 7-14 days (see retention-days in workflow)
- Re-run workflow if needed

## Performance Optimization

Current optimizations:
- Cargo and npm dependency caching
- Parallel job execution where possible
- Selective test runs (only related tests)
- Artifact compression and retention management

## Extending Workflows

To add new jobs or tests:

1. **Edit workflow file** (e.g., ci.yml)
2. **Add new job**:
   ```yaml
   new-test-job:
     name: My New Test
     runs-on: ubuntu-latest
     needs: [build-frontend]  # Dependencies
     steps:
       - name: Checkout
         uses: actions/checkout@v4
       # Add your steps
   ```
3. **Update dependencies** in other jobs if needed
4. **Test locally** if possible
5. **Commit and push** to trigger workflow
6. **Review results** and iterate

## Best Practices

1. **Keep secrets secure**: Never log secret values, use `${{ secrets.NAME }}` syntax
2. **Use caching**: Cache dependencies to speed up workflows
3. **Fail fast**: Configure tests to fail quickly on critical errors
4. **Artifact management**: Set appropriate retention days for artifacts
5. **Mobile-friendly**: Ensure artifacts are viewable/usable on mobile devices
6. **Documentation**: Document any workflow changes in this README

## Resources

- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Workflow Syntax](https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions)
- [Secrets Management](https://docs.github.com/en/actions/security-guides/encrypted-secrets)
- [Repository Setup Guide](../../GITHUB_SECRETS_SETUP.md)
