#!/usr/bin/env node

/**
 * Debug bot functionality with Playwright
 * This script connects to the app, captures console logs, and tests the Shyft integration
 */

import { chromium } from 'playwright';

async function debugBot() {
  console.log('=== Starting Sol Beast Debug Session ===\n');
  
  const browser = await chromium.launch({
    headless: true,
    args: [
      '--disable-web-security',
      '--disable-features=IsolateOrigins,site-per-process',
      '--no-sandbox',
      '--disable-dev-shm-usage'
    ]
  });
  
  const context = await browser.newContext({
    viewport: { width: 1920, height: 1080 },
    ignoreHTTPSErrors: true
  });
  
  const page = await context.newPage();
  
  // Storage for console messages
  const logs = {
    info: [],
    error: [],
    warn: [],
    log: []
  };
  
  // Capture all console messages
  page.on('console', msg => {
    const text = msg.text();
    const type = msg.type();
    const timestamp = new Date().toISOString();
    
    const entry = `[${timestamp}] [${type.toUpperCase()}] ${text}`;
    console.log(entry);
    
    if (logs[type]) {
      logs[type].push(entry);
    }
  });
  
  // Capture page errors
  page.on('pageerror', error => {
    const entry = `[PAGE ERROR] ${error.message}\n${error.stack}`;
    console.error(entry);
    logs.error.push(entry);
  });
  
  // Capture network errors
  page.on('requestfailed', request => {
    console.log(`[NETWORK ERROR] ${request.url()} - ${request.failure()?.errorText}`);
  });
  
  try {
    console.log('Navigating to http://localhost:3000...\n');
    await page.goto('http://localhost:3000', { waitUntil: 'networkidle', timeout: 30000 });
    
    // Wait for the app to load
    await page.waitForSelector('body', { timeout: 5000 });
    
    // Take a screenshot
    await page.screenshot({ path: '/workspaces/sol_beast/debug-initial.png', fullPage: true });
    console.log('\n✓ Initial screenshot saved to debug-initial.png\n');
    
    // Get page title
    const title = await page.title();
    console.log(`Page title: ${title}\n`);
    
    // Check if we're in WASM mode
    const wasmMode = await page.evaluate(() => {
      return window.location.hostname.endsWith('.github.io') || 
             (typeof process !== 'undefined' && process.env?.VITE_USE_WASM === 'true');
    }).catch(() => false);
    
    console.log(`WASM Mode: ${wasmMode}\n`);
    
    // Wait a bit for the app to initialize
    await page.waitForTimeout(3000);
    
    // Check for Configuration button/panel
    const hasConfigPanel = await page.locator('text=Configuration').count() > 0;
    console.log(`Configuration panel found: ${hasConfigPanel}\n`);
    
    if (hasConfigPanel) {
      console.log('Clicking on Configuration...\n');
      await page.click('text=Configuration');
      await page.waitForTimeout(1000);
      await page.screenshot({ path: '/workspaces/sol_beast/debug-config.png', fullPage: true });
      console.log('✓ Configuration screenshot saved\n');
    }
    
    // Check for bot settings in localStorage
    const settings = await page.evaluate(() => {
      const botState = localStorage.getItem('solBeastBotState');
      return botState ? JSON.parse(botState) : null;
    });
    
    console.log('Bot Settings from localStorage:');
    console.log(JSON.stringify(settings, null, 2));
    console.log('\n');
    
    // Try to update settings with Shyft API key
    console.log('Updating settings with Shyft API key...\n');
    await page.evaluate(() => {
      const shyftApiKey = 'ULYlbsBOcBGDjY-a';
      const shyftGraphqlUrl = 'https://programs.shyft.to/v0/graphql/?network=mainnet-beta';
      const shyftWsUrl = 'wss://rpc.shyft.to';
      
      // Try to update via window API if available
      if (window.updateBotSettings) {
        window.updateBotSettings({
          shyft_api_key: shyftApiKey,
          shyft_graphql_url: shyftGraphqlUrl,
          solana_ws_urls: [shyftWsUrl]
        });
      }
    });
    
    await page.waitForTimeout(2000);
    
    // Look for Start Bot button
    const startButton = page.locator('button:has-text("Start Bot")').or(page.locator('button:has-text("Start")'));
    const hasStartButton = await startButton.count() > 0;
    console.log(`Start button found: ${hasStartButton}\n`);
    
    if (hasStartButton) {
      console.log('Starting the bot...\n');
      await startButton.first().click();
      await page.waitForTimeout(2000);
      
      // Take screenshot after starting
      await page.screenshot({ path: '/workspaces/sol_beast/debug-started.png', fullPage: true });
      console.log('✓ Bot started screenshot saved\n');
      
      // Monitor for 30 seconds
      console.log('Monitoring for 30 seconds...\n');
      await page.waitForTimeout(30000);
      
      // Final screenshot
      await page.screenshot({ path: '/workspaces/sol_beast/debug-final.png', fullPage: true });
      console.log('✓ Final screenshot saved\n');
    }
    
    // Summary
    console.log('\n=== Debug Session Summary ===');
    console.log(`Total console.log messages: ${logs.log.length}`);
    console.log(`Total console.info messages: ${logs.info.length}`);
    console.log(`Total console.warn messages: ${logs.warn.length}`);
    console.log(`Total console.error messages: ${logs.error.length}`);
    
    if (logs.error.length > 0) {
      console.log('\nERRORS:');
      logs.error.forEach(err => console.log(err));
    }
    
  } catch (error) {
    console.error('Debug session failed:', error);
  }
  
  // Close browser
  console.log('\nClosing browser...');
  await browser.close();
}

debugBot().catch(console.error);
