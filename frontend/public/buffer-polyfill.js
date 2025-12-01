// Buffer polyfill for Solana Web3.js - using feross/buffer implementation
// This must be loaded as a regular script before any ES modules
(function() {
  'use strict';
  
  // Check if Buffer is already defined
  if (typeof window.Buffer !== 'undefined') {
    console.log('Buffer already defined');
    return;
  }
  
  // We'll use a bundled version of the buffer package
  // For now, set up a placeholder that will be replaced by the actual implementation
  // The rollup-plugin-polyfill-node should inject the real Buffer into modules
  
  // Set global for compatibility
  window.global = window;
  
  // Import will happen in modules, but we need to ensure window.Buffer exists
  // as a fallback. The modules will use the rollup-injected Buffer.
  console.log('Buffer polyfill placeholder loaded, waiting for module initialization');
})();
