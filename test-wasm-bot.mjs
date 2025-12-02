/**
 * Simple test script for WASM bot initialization
 * Tests that the bot can be created and get_settings() doesn't throw "unreachable"
 */

import { readFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Mock browser globals for Node.js environment
global.window = {};
global.TextDecoder = TextDecoder;
global.TextEncoder = TextEncoder;

console.log('🧪 Testing WASM Bot Module\n');
console.log('='.repeat(60));

try {
  console.log('\n✓ Step 1: Loading WASM module...');
  const wasmPath = join(__dirname, 'frontend/src/wasm/sol_beast_wasm_bg.wasm');
  const wasmBuffer = readFileSync(wasmPath);
  
  console.log(`  - WASM file size: ${(wasmBuffer.length / 1024).toFixed(2)} KB`);
  console.log('  - WASM file loaded successfully');
  
  console.log('\n✓ Step 2: Loading JavaScript bindings...');
  const jsPath = join(__dirname, 'frontend/src/wasm/sol_beast_wasm.js');
  
  // Import the WASM module
  const wasmModule = await import(jsPath);
  console.log('  - JavaScript bindings loaded');
  
  console.log('\n✓ Step 3: Initializing WASM runtime...');
  await wasmModule.default(wasmBuffer);
  console.log('  - WASM runtime initialized');
  
  console.log('\n✓ Step 4: Creating bot instance...');
  const bot = new wasmModule.SolBeastBot();
  console.log('  - Bot instance created successfully');
  
  console.log('\n✓ Step 5: Testing get_settings() method...');
  try {
    const settingsJson = bot.get_settings();
    console.log('  - get_settings() called successfully');
    
    const settings = JSON.parse(settingsJson);
    console.log('  - Settings parsed successfully');
    console.log(`  - WebSocket URLs: ${settings.solana_ws_urls?.length || 0}`);
    console.log(`  - RPC URLs: ${settings.solana_rpc_urls?.length || 0}`);
    console.log(`  - Buy amount: ${settings.buy_amount}`);
    console.log(`  - TP percent: ${settings.tp_percent}%`);
    console.log(`  - SL percent: ${settings.sl_percent}%`);
  } catch (error) {
    console.error('\n✗ FAILED: get_settings() threw an error!');
    console.error(`  Error: ${error.message || error}`);
    console.error('\n  This indicates the "unreachable" bug is still present.');
    throw error;
  }
  
  console.log('\n✓ Step 6: Testing bot state methods...');
  const isRunning = bot.is_running();
  console.log(`  - is_running(): ${isRunning}`);
  
  const mode = bot.get_mode();
  console.log(`  - get_mode(): ${mode}`);
  
  console.log('\n' + '='.repeat(60));
  console.log('✅ ALL TESTS PASSED!');
  console.log('='.repeat(60));
  console.log('\nThe WASM module is working correctly:');
  console.log('  ✓ No "unreachable" errors');
  console.log('  ✓ Bot can be initialized');
  console.log('  ✓ Settings can be retrieved');
  console.log('  ✓ State methods work correctly');
  console.log('\nThe bot should now start successfully in the browser! 🎉\n');
  
  process.exit(0);
  
} catch (error) {
  console.error('\n' + '='.repeat(60));
  console.error('❌ TEST FAILED');
  console.error('='.repeat(60));
  console.error(`\nError: ${error.message || error}`);
  console.error('\nStack trace:');
  console.error(error.stack);
  console.error('\nThe WASM module has issues that need to be fixed.\n');
  
  process.exit(1);
}
