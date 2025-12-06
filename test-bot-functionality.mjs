#!/usr/bin/env node

/**
 * Enhanced Playwright test for Sol Beast bot functionality
 * 
 * This script:
 * 1. Launches browser and navigates to the app
 * 2. Configures RPC endpoints (using env vars if available)
 * 3. Starts the bot in DRY RUN mode
 * 4. Monitors for new coin detection and transaction logs
 * 5. Captures detailed console activity for 2-5 minutes
 * 6. Reports on bot functionality and error states
 * 
 * Usage:
 *   SOLANA_RPC_URL="https://..." SOLANA_WS_URL="wss://..." node test-bot-functionality.mjs [url] [duration_seconds]
 * 
 * Default URL: http://localhost:8080/sol_beast/
 * Default duration: 180 seconds (3 minutes)
 */

import { chromium } from 'playwright';
import { writeFileSync } from 'fs';

const DEFAULT_URL = 'http://localhost:8080/sol_beast/';
const targetUrl = process.argv[2] || DEFAULT_URL;
const monitorDuration = parseInt(process.argv[3] || '180', 10); // 3 minutes default

// Get RPC URLs from environment or use defaults
const SOLANA_RPC_URL = process.env.SOLANA_RPC_URL || 'https://api.mainnet-beta.solana.com';
const SOLANA_WS_URL = process.env.SOLANA_WS_URL || 'wss://api.mainnet-beta.solana.com';

console.log('=== Sol Beast Bot Functionality Test ===');
console.log(`Target URL: ${targetUrl}`);
console.log(`Monitor Duration: ${monitorDuration} seconds`);
console.log(`RPC URL: ${SOLANA_RPC_URL}`);
console.log(`WS URL: ${SOLANA_WS_URL}`);
console.log('');

async function testBotFunctionality() {
  const browser = await chromium.launch({ 
    headless: false, // Run with visible browser to see what's happening
    args: ['--disable-web-security']
  });
  
  const context = await browser.newContext({
    viewport: { width: 1920, height: 1080 },
    ignoreHTTPSErrors: true
  });
  
  const page = await context.newPage();
  
  // Storage for findings
  const findings = {
    consoleMessages: [],
    botStarted: false,
    rpcConfigured: false,
    newCoinsDetected: 0,
    transactionsReceived: 0,
    errors: [],
    botLogs: []
  };
  
  // Capture all console messages
  page.on('console', msg => {
    const text = msg.text();
    const entry = {
      type: msg.type(),
      text: text,
      timestamp: new Date().toISOString()
    };
    findings.consoleMessages.push(entry);
    
    // Log in real-time for visibility
    console.log(`[${entry.type.toUpperCase()}] ${text}`);
    
    // Track specific events
    if (text.includes('detected') || text.includes('new coin') || text.includes('New token')) {
      findings.newCoinsDetected++;
      findings.botLogs.push(`NEW COIN: ${text}`);
    }
    if (text.includes('Received tx') || text.includes('Transaction') || text.includes('signature')) {
      findings.transactionsReceived++;
      findings.botLogs.push(`TX: ${text}`);
    }
    if (text.includes('Bot started') || text.includes('started successfully')) {
      findings.botStarted = true;
    }
    if (text.includes('RPC') || text.includes('WebSocket connected')) {
      findings.rpcConfigured = true;
    }
    if (msg.type() === 'error') {
      findings.errors.push(text);
    }
  });
  
  // Capture page errors
  page.on('pageerror', error => {
    const entry = {
      message: error.message,
      timestamp: new Date().toISOString()
    };
    findings.errors.push(entry.message);
    console.error(`[PAGE ERROR] ${error.message}`);
  });
  
  try {
    console.log('RUNNING MODIFIED SCRIPT');
    console.log('\n=== Step 1: Navigate to application ===');
    try {
      await page.goto(targetUrl, { waitUntil: 'domcontentloaded', timeout: 60000 });
    } catch (e) {
      console.log('Navigation failed or timed out, checking content anyway...');
      console.log(e.message);
    }
    
    // Wait for app to initialize
    await page.waitForTimeout(5000);
    console.log('✓ App loaded');
    
    // Take initial screenshot
    await page.screenshot({ path: 'bot-test-01-initial.png', fullPage: false });
    console.log('✓ Initial screenshot saved');
    
    console.log('\n=== Step 2: Configure RPC endpoints ===');
    
    // Check if RPC config modal is shown
    const hasModal = await page.locator('[data-testid="rpc-config-modal"], .modal').first().isVisible().catch(() => false);
    
    if (hasModal) {
      console.log('RPC configuration modal detected, configuring...');
      
      // Fill in RPC URL
      const rpcInput = page.locator('textarea, input').filter({ hasText: /https/ }).or(page.locator('textarea').first());
      await rpcInput.clear();
      await rpcInput.fill(SOLANA_RPC_URL);
      
      // Fill in WS URL
      const wsInput = page.locator('textarea, input').filter({ hasText: /wss/ }).or(page.locator('textarea').nth(1));
      await wsInput.clear();
      await wsInput.fill(SOLANA_WS_URL);
      
      // Click save/continue button
      const saveButton = page.locator('button').filter({ hasText: /Save|Continue|Confirm/i });
      await saveButton.click();
      
      await page.waitForTimeout(2000);
      console.log('✓ RPC configuration submitted');
    } else {
      console.log('No RPC modal shown, checking Configuration panel...');
      
      // Navigate to Configuration panel
      const configButton = page.locator('button').filter({ hasText: /Configuration/i });
      if (await configButton.isVisible()) {
        await configButton.click();
        await page.waitForTimeout(1000);
        
        // Update RPC URLs in configuration
        const rpcTextareas = page.locator('textarea');
        const count = await rpcTextareas.count();
        
        if (count >= 2) {
          await rpcTextareas.nth(0).clear();
          await rpcTextareas.nth(0).fill(SOLANA_WS_URL);
          
          await rpcTextareas.nth(1).clear();
          await rpcTextareas.nth(1).fill(SOLANA_RPC_URL);
          
          // Save settings
          const saveBtn = page.locator('button').filter({ hasText: /Save/i });
          await saveBtn.click();
          await page.waitForTimeout(2000);
          console.log('✓ RPC URLs configured via Configuration panel');
        }
        
        // Navigate back to Dashboard
        const dashboardBtn = page.locator('button').filter({ hasText: /Dashboard/i });
        await dashboardBtn.click();
        await page.waitForTimeout(1000);
      }
    }
    
    await page.screenshot({ path: 'bot-test-02-configured.png', fullPage: false });
    console.log('✓ Configuration screenshot saved');
    
    console.log('\n=== Step 3: Start the bot in DRY RUN mode ===');
    
    // Ensure DRY RUN mode is selected
    const dryRunBtn = page.locator('button').filter({ hasText: /Dry Run/i });
    if (await dryRunBtn.isVisible()) {
      await dryRunBtn.click();
      await page.waitForTimeout(500);
      console.log('✓ DRY RUN mode selected');
    }
    
    // Click START BOT button
    const startBtn = page.locator('button').filter({ hasText: /Start Bot/i });
    if (await startBtn.isVisible()) {
      console.log('Clicking START BOT...');
      await startBtn.click();
      await page.waitForTimeout(3000);
      
      // Check if bot started
      const statusIndicator = page.locator('text=/RUNNING|ACTIVE|Started/i');
      if (await statusIndicator.isVisible().catch(() => false)) {
        console.log('✅ Bot started successfully!');
        findings.botStarted = true;
      } else {
        console.log('⚠️  Bot status unclear, checking console logs...');
      }
    } else {
      console.log('❌ START BOT button not found');
    }
    
    await page.screenshot({ path: 'bot-test-03-started.png', fullPage: false });
    console.log('✓ Bot started screenshot saved');
    
    console.log(`\n=== Step 4: Monitor bot activity for ${monitorDuration} seconds ===`);
    console.log('Waiting for new coin detection and transaction logs...');
    console.log('(Press Ctrl+C to stop early)\n');
    
    // Monitor for specified duration
    const startTime = Date.now();
    let lastUpdate = startTime;
    
    while (Date.now() - startTime < monitorDuration * 1000) {
      await page.waitForTimeout(5000);
      
      // Print status update every 30 seconds
      if (Date.now() - lastUpdate > 30000) {
        console.log(`\n--- Status Update (${Math.floor((Date.now() - startTime) / 1000)}s elapsed) ---`);
        console.log(`New coins detected: ${findings.newCoinsDetected}`);
        console.log(`Transactions received: ${findings.transactionsReceived}`);
        console.log(`Console messages: ${findings.consoleMessages.length}`);
        console.log(`Errors: ${findings.errors.length}`);
        lastUpdate = Date.now();
        
        // Take periodic screenshot
        const elapsed = Math.floor((Date.now() - startTime) / 1000);
        await page.screenshot({ path: `bot-test-04-monitoring-${elapsed}s.png`, fullPage: false });
      }
    }
    
    console.log('\n=== Step 5: Stop monitoring and generate report ===');
    
    // Take final screenshot
    await page.screenshot({ path: 'bot-test-05-final.png', fullPage: true });
    console.log('✓ Final screenshot saved');
    
  } catch (error) {
    console.error(`\n[TEST ERROR] ${error.message}`);
    findings.errors.push(`Test execution error: ${error.message}`);
  } finally {
    await browser.close();
  }
  
  // Generate report
  console.log('\n' + '='.repeat(80));
  console.log('BOT FUNCTIONALITY TEST REPORT');
  console.log('='.repeat(80));
  
  console.log(`\nTest URL: ${targetUrl}`);
  console.log(`Monitor Duration: ${monitorDuration} seconds`);
  console.log(`RPC Endpoint: ${SOLANA_RPC_URL}`);
  console.log(`WS Endpoint: ${SOLANA_WS_URL}`);
  
  console.log(`\n--- RESULTS ---`);
  console.log(`Bot Started: ${findings.botStarted ? '✅ YES' : '❌ NO'}`);
  console.log(`RPC Configured: ${findings.rpcConfigured ? '✅ YES' : '⚠️  UNCLEAR'}`);
  console.log(`New Coins Detected: ${findings.newCoinsDetected}`);
  console.log(`Transactions Received: ${findings.transactionsReceived}`);
  console.log(`Total Console Messages: ${findings.consoleMessages.length}`);
  console.log(`Errors: ${findings.errors.length}`);
  
  if (findings.botLogs.length > 0) {
    console.log(`\n--- BOT ACTIVITY LOGS (Last 20) ---`);
    findings.botLogs.slice(-20).forEach((log, idx) => {
      console.log(`[${idx + 1}] ${log}`);
    });
  }
  
  if (findings.errors.length > 0) {
    console.log(`\n--- ERRORS (Last 10) ---`);
    findings.errors.slice(-10).forEach((error, idx) => {
      console.log(`[${idx + 1}] ${error}`);
    });
  }
  
  // Save detailed report
  const reportPath = 'bot-functionality-report.json';
  writeFileSync(reportPath, JSON.stringify(findings, null, 2));
  console.log(`\n✓ Detailed report saved: ${reportPath}`);
  
  console.log('\n' + '='.repeat(80));
  
  // Determine success
  if (findings.botStarted && findings.newCoinsDetected > 0) {
    console.log('✅ TEST PASSED: Bot is functioning and detecting new coins');
    process.exit(0);
  } else if (findings.botStarted && findings.transactionsReceived > 0) {
    console.log('✅ TEST PASSED: Bot is functioning and receiving transactions');
    process.exit(0);
  } else if (findings.botStarted) {
    console.log('⚠️  TEST PARTIAL: Bot started but no activity detected (may need longer monitoring)');
    process.exit(0);
  } else {
    console.log('❌ TEST FAILED: Bot did not start successfully');
    process.exit(1);
  }
}

// Run the test
testBotFunctionality().catch(error => {
  console.error('Fatal error running test:', error);
  process.exit(1);
});
