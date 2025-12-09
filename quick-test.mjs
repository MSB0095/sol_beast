#!/usr/bin/env node

import { chromium } from 'playwright';

async function quickTest() {
  console.log('=== Quick Sol Beast Test ===\n');
  
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  
  let testPassed = false;
  let botStarted = false;
  let wsConnected = false;
  
  // Capture console messages
  page.on('console', msg => {
    const text = msg.text();
    console.log(`[${msg.type()}] ${text}`);
    
    if (text.includes('Bot started successfully') || text.includes('✓ Bot started')) {
      botStarted = true;
    }
    if (text.includes('WebSocket connected') || text.includes('Subscription confirmed')) {
      wsConnected = true;
    }
  });
  
  try {
    console.log('Loading app...');
    await page.goto('http://localhost:3000', { waitUntil: 'networkidle', timeout: 20000 });
    
    console.log('Waiting for initialization...');
    await page.waitForTimeout(3000);
    
    console.log('Looking for start button...');
    const startButton = page.locator('button:has-text("Start Bot"), button:has-text("Start")');
    const hasButton = await startButton.count() > 0;
    
    if (!hasButton) {
      console.log('❌ Start button not found!');
      await browser.close();
      return;
    }
    
    console.log('✓ Start button found, clicking...');
    await startButton.first().click({ timeout: 5000 });
    
    console.log('Waiting for bot to start (30 seconds)...');
    await page.waitForTimeout(30000);
    
    if (botStarted) {
      console.log('\n✅ SUCCESS: Bot started successfully!');
      testPassed = true;
      
      if (wsConnected) {
        console.log('✅ BONUS: WebSocket connected!');
      } else {
        console.log('⚠️  WebSocket not yet confirmed (may need more time)');
      }
    } else {
      console.log('\n❌ FAILED: Bot did not start within 30 seconds');
    }
    
  } catch (error) {
    console.error('\n❌ Test failed with error:', error.message);
  }
  
  await browser.close();
  
  if (testPassed) {
    console.log('\n🎉 Test PASSED! The bot is working in WASM mode with Shyft.');
    process.exit(0);
  } else {
    console.log('\n❌ Test FAILED');
    process.exit(1);
  }
}

quickTest().catch(console.error);
