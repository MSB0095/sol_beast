#!/usr/bin/env node

/**
 * Playwright test script for Sol Beast GitHub Pages deployment
 * 
 * This script:
 * 1. Launches a browser with Playwright
 * 2. Navigates to the local deployment at http://localhost:8080/sol_beast/
 * 3. Captures all console messages and errors
 * 4. Takes screenshots
 * 5. Reports all findings
 * 
 * Usage:
 *   node test-with-playwright.mjs [url]
 * 
 * Default URL: http://localhost:8080/sol_beast/
 */

import { chromium } from 'playwright';
import { writeFileSync } from 'fs';

const DEFAULT_URL = 'http://localhost:8080/sol_beast/';
const targetUrl = process.argv[2] || DEFAULT_URL;

console.log('=== Sol Beast Deployment Testing with Playwright ===');
console.log(`Target URL: ${targetUrl}`);
console.log('');

async function testDeployment() {
  const browser = await chromium.launch({ 
    headless: true,
    args: ['--disable-web-security'] // Allow CORS for testing
  });
  
  const context = await browser.newContext({
    viewport: { width: 1920, height: 1080 },
    ignoreHTTPSErrors: true
  });
  
  const page = await context.newPage();
  
  // Storage for all findings
  const findings = {
    consoleMessages: [],
    pageErrors: [],
    networkErrors: [],
    resourceLoadErrors: []
  };
  
  // Capture console messages
  page.on('console', msg => {
    const entry = {
      type: msg.type(),
      text: msg.text(),
      location: msg.location(),
      timestamp: new Date().toISOString()
    };
    findings.consoleMessages.push(entry);
    
    // Print in real-time for visibility
    const prefix = `[${entry.type.toUpperCase()}]`;
    console.log(`${prefix} ${entry.text}`);
    if (entry.location && entry.location.url) {
      console.log(`  @ ${entry.location.url}:${entry.location.lineNumber}:${entry.location.columnNumber}`);
    }
  });
  
  // Capture page errors (uncaught exceptions)
  page.on('pageerror', error => {
    const entry = {
      message: error.message,
      stack: error.stack,
      timestamp: new Date().toISOString()
    };
    findings.pageErrors.push(entry);
    console.error(`[PAGE ERROR] ${error.message}`);
    if (error.stack) {
      console.error(error.stack);
    }
  });
  
  // Capture failed network requests
  page.on('requestfailed', request => {
    const entry = {
      url: request.url(),
      method: request.method(),
      resourceType: request.resourceType(),
      failure: request.failure()?.errorText || 'Unknown error',
      timestamp: new Date().toISOString()
    };
    findings.networkErrors.push(entry);
    console.error(`[NETWORK ERROR] ${entry.method} ${entry.url}`);
    console.error(`  Failure: ${entry.failure}`);
  });
  
  // Capture response errors (404, 500, etc.)
  page.on('response', response => {
    if (!response.ok()) {
      const entry = {
        url: response.url(),
        status: response.status(),
        statusText: response.statusText(),
        timestamp: new Date().toISOString()
      };
      findings.resourceLoadErrors.push(entry);
      console.error(`[RESOURCE ERROR] ${entry.status} ${entry.statusText} - ${entry.url}`);
    }
  });
  
  try {
    console.log('\nNavigating to the application...');
    await page.goto(targetUrl, { 
      waitUntil: 'networkidle', 
      timeout: 60000 
    });
    console.log('Navigation complete. Waiting for app initialization...\n');
    
    // Wait for the app to initialize
    await page.waitForTimeout(10000);
    
    // Take screenshots
    console.log('\nTaking screenshots...');
    await page.screenshot({ 
      path: 'deployment-test-full.png', 
      fullPage: true 
    });
    console.log('✓ Full page screenshot saved: deployment-test-full.png');
    
    await page.screenshot({ 
      path: 'deployment-test-viewport.png', 
      fullPage: false 
    });
    console.log('✓ Viewport screenshot saved: deployment-test-viewport.png');
    
    // Try to interact with the app to trigger more potential errors
    console.log('\nChecking for interactive elements...');
    
    // Check if RPC config modal is shown
    const rpcModal = await page.locator('[data-testid="rpc-config-modal"], .modal, [role="dialog"]').first();
    const isModalVisible = await rpcModal.isVisible().catch(() => false);
    
    if (isModalVisible) {
      console.log('✓ RPC Configuration modal detected');
      await page.screenshot({ 
        path: 'deployment-test-rpc-modal.png', 
        fullPage: false 
      });
      console.log('✓ RPC modal screenshot saved: deployment-test-rpc-modal.png');
    } else {
      console.log('ℹ No modal detected on initial load');
    }
    
    // Wait a bit more to capture any delayed errors
    await page.waitForTimeout(5000);
    
  } catch (error) {
    console.error(`\n[TEST ERROR] Failed to complete test: ${error.message}`);
    findings.pageErrors.push({
      message: `Test execution error: ${error.message}`,
      stack: error.stack,
      timestamp: new Date().toISOString()
    });
  } finally {
    await browser.close();
  }
  
  // Generate report
  console.log('\n' + '='.repeat(80));
  console.log('DEPLOYMENT TEST REPORT');
  console.log('='.repeat(80));
  
  console.log(`\nTest URL: ${targetUrl}`);
  console.log(`Test completed at: ${new Date().toISOString()}`);
  
  console.log(`\n--- SUMMARY ---`);
  console.log(`Total console messages: ${findings.consoleMessages.length}`);
  console.log(`  - Errors: ${findings.consoleMessages.filter(m => m.type === 'error').length}`);
  console.log(`  - Warnings: ${findings.consoleMessages.filter(m => m.type === 'warning').length}`);
  console.log(`  - Info: ${findings.consoleMessages.filter(m => m.type === 'info' || m.type === 'log').length}`);
  console.log(`Page errors (uncaught exceptions): ${findings.pageErrors.length}`);
  console.log(`Network failures: ${findings.networkErrors.length}`);
  console.log(`Resource load errors (404, 500, etc.): ${findings.resourceLoadErrors.length}`);
  
  // Detailed error listings
  if (findings.consoleMessages.filter(m => m.type === 'error').length > 0) {
    console.log(`\n--- CONSOLE ERRORS ---`);
    findings.consoleMessages
      .filter(m => m.type === 'error')
      .forEach((msg, idx) => {
        console.log(`\n[${idx + 1}] ${msg.text}`);
        if (msg.location && msg.location.url) {
          console.log(`    @ ${msg.location.url}:${msg.location.lineNumber}:${msg.location.columnNumber}`);
        }
      });
  }
  
  if (findings.pageErrors.length > 0) {
    console.log(`\n--- PAGE ERRORS ---`);
    findings.pageErrors.forEach((error, idx) => {
      console.log(`\n[${idx + 1}] ${error.message}`);
      if (error.stack) {
        console.log(error.stack.split('\n').slice(0, 5).join('\n'));
      }
    });
  }
  
  if (findings.networkErrors.length > 0) {
    console.log(`\n--- NETWORK ERRORS ---`);
    findings.networkErrors.forEach((error, idx) => {
      console.log(`\n[${idx + 1}] ${error.method} ${error.url}`);
      console.log(`    Failure: ${error.failure}`);
    });
  }
  
  if (findings.resourceLoadErrors.length > 0) {
    console.log(`\n--- RESOURCE LOAD ERRORS ---`);
    findings.resourceLoadErrors.forEach((error, idx) => {
      console.log(`\n[${idx + 1}] ${error.status} ${error.statusText}`);
      console.log(`    ${error.url}`);
    });
  }
  
  // Save detailed report to JSON
  const reportPath = 'deployment-test-report.json';
  writeFileSync(reportPath, JSON.stringify(findings, null, 2));
  console.log(`\n✓ Detailed report saved: ${reportPath}`);
  
  console.log('\n' + '='.repeat(80));
  
  // Determine test result - filter out expected/non-critical errors
  const criticalErrors = findings.consoleMessages.filter(m => {
    if (m.type !== 'error') return false;
    const text = m.text.toLowerCase();
    // Ignore external CDN failures (expected in some environments)
    if (text.includes('iconify.design') || text.includes('googleapis.com')) return false;
    // Ignore expected backend API 404s
    if (text.includes('/health') || text.includes('/settings')) return false;
    // Ignore development resource references
    if (text.includes('/src/main.tsx') || text.includes('vite.svg')) return false;
    return true;
  });
  
  const hasCriticalErrors = criticalErrors.length > 0 || findings.pageErrors.length > 0;
  
  if (hasCriticalErrors) {
    console.log('❌ TEST FAILED: Critical errors or exceptions detected');
    console.log(`   Critical console errors: ${criticalErrors.length}`);
    console.log(`   Page exceptions: ${findings.pageErrors.length}`);
    process.exit(1);
  } else {
    console.log('✅ TEST PASSED: No critical errors detected');
    console.log(`   Total console errors (including non-critical): ${findings.consoleMessages.filter(m => m.type === 'error').length}`);
    console.log(`   Critical errors: 0`);
    process.exit(0);
  }
}

// Run the test
testDeployment().catch(error => {
  console.error('Fatal error running test:', error);
  process.exit(1);
});
