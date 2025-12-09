#!/usr/bin/env node
import { chromium } from 'playwright';

async function quickTest() {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  
  // Capture console
  page.on('console', msg => console.log(`[${msg.type()}] ${msg.text()}`));
  
  try {
    await page.goto('http://localhost:3000', { timeout: 10000 });
    await page.waitForTimeout(5000);
    
    console.log('\n=== Clicking Start Bot ===\n');
    
    // Set a shorter timeout and handle errors
    try {
      await page.click('button:has-text("Start")', { timeout: 5000 });
      console.log('✓ Click completed');
    } catch (e) {
      console.error('✗ Click timed out or failed:', e.message);
    }
    
    await page.waitForTimeout(5000);
    console.log('\n=== Test Complete ===');
    
  } catch (error) {
    console.error('Test failed:', error.message);
  } finally {
    await browser.close();
  }
}

quickTest().catch(console.error);
