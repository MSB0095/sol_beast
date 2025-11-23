# GitHub Pages Setup Guide

This guide explains how to set up and use the GitHub Pages deployment for Sol Beast.

## Overview

Sol Beast uses GitHub Actions to automatically build and deploy the frontend application to GitHub Pages. The deployment process includes:

1. Building the WASM module from Rust code
2. Building the React frontend
3. Deploying to GitHub Pages

## Workflow Configuration

The deployment workflow is defined in `.github/workflows/gh-pages.yml` and runs automatically when:
- Code is pushed to the `main` or `master` branch
- Manually triggered via workflow dispatch

### Build Steps

1. **Setup Rust**: Installs Rust toolchain with `wasm32-unknown-unknown` target
2. **Install wasm-pack**: Installs the wasm-pack tool for building WASM modules
3. **Cache Dependencies**: Caches Rust and npm dependencies for faster builds
4. **Build WASM**: Compiles the `sol_beast_wasm` crate to WebAssembly
5. **Setup Node.js**: Installs Node.js 18 for building the frontend
6. **Install Dependencies**: Installs npm packages
7. **Build Frontend**: Builds the React app with production settings
8. **Deploy**: Uploads and deploys the built files to GitHub Pages

## Repository Settings

To enable GitHub Pages deployment, configure these settings in your GitHub repository:

1. Go to **Settings** → **Pages**
2. Under **Source**, select **GitHub Actions**
3. The workflow will automatically create and deploy to the `gh-pages` branch

## Base Path Configuration

The frontend is configured to work with GitHub Pages' base path:

```typescript
// vite.config.ts
base: process.env.NODE_ENV === 'production' ? '/sol_beast/' : '/'
```

This ensures that all assets and routes work correctly when deployed to `https://your-username.github.io/sol_beast/`.

### Custom Domain (Optional)

To use a custom domain:

1. Add a `CNAME` file to `frontend/public/` with your domain name
2. Configure your DNS provider to point to GitHub Pages
3. Update the `base` path in `vite.config.ts` to `/` for custom domains

## Local Development

For local development, the app runs with base path `/`:

```bash
cd frontend
npm install
npm run dev
```

Visit `http://localhost:3000` to see the app running locally.

## Manual Deployment

To manually trigger a deployment:

1. Go to **Actions** tab in GitHub
2. Select **Deploy to GitHub Pages** workflow
3. Click **Run workflow**
4. Select the branch and click **Run workflow**

## Troubleshooting

### Build Failures

If the workflow fails, check:

1. **Rust compilation errors**: Review the "Build WASM module" step logs
2. **TypeScript errors**: Review the "Build frontend" step logs
3. **Dependency issues**: Clear caches by re-running the workflow

### Deployment Issues

If the site doesn't load correctly:

1. Verify GitHub Pages is enabled in repository settings
2. Check that the base path in `vite.config.ts` matches your deployment URL
3. Look for console errors in browser developer tools
4. Ensure `.nojekyll` file is present in the deployed files

### WASM Loading Issues

If WASM module fails to load:

1. Check browser console for CORS or loading errors
2. Verify WASM files are present in the deployed site
3. Check that the WASM build completed successfully in the workflow
4. Ensure `src/wasm/` directory is properly generated during build

## Files and Directories

### Workflow Files
- `.github/workflows/gh-pages.yml` - Main deployment workflow

### Frontend Configuration
- `frontend/vite.config.ts` - Vite configuration with base path
- `frontend/package.json` - Dependencies and build scripts
- `frontend/public/.nojekyll` - Disables Jekyll processing

### WASM Build
- `sol_beast_wasm/` - WASM crate source
- `sol_beast_wasm/wasm-pack-build.sh` - Local build script
- `frontend/src/wasm/` - Generated WASM files (gitignored)

## Build Artifacts

After a successful build, the following files are deployed:

```
frontend/dist/
├── index.html
├── assets/
│   ├── *.js
│   ├── *.css
│   └── *.wasm
├── .nojekyll
└── README.txt
```

## Performance Considerations

- WASM files are optimized for size with `opt-level = "z"`
- Rust dependencies are cached between builds
- npm dependencies are cached using package-lock.json
- Build artifacts are compressed before upload

## Security

- The workflow uses GitHub's OIDC token for deployment
- No secrets are required for deployment
- Source code and dependencies are verified during checkout
- WASM provides sandboxed execution in the browser

## Monitoring

To monitor deployments:

1. Check the **Actions** tab for workflow run status
2. View deployment logs for detailed information
3. Visit the deployment URL to verify the site is working
4. Use browser developer tools to check for errors

## Updating the Workflow

To modify the deployment process:

1. Edit `.github/workflows/gh-pages.yml`
2. Test changes in a feature branch first
3. Monitor the workflow run after merging
4. Revert if issues occur using GitHub's revert feature

## Additional Resources

- [GitHub Pages Documentation](https://docs.github.com/en/pages)
- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [wasm-pack Documentation](https://rustwasm.github.io/wasm-pack/)
- [Vite Documentation](https://vitejs.dev/)
