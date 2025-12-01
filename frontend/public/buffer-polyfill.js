// Minimal Buffer polyfill for Solana Web3.js
// This creates a basic Buffer implementation that's sufficient for the libraries to load
(function() {
  'use strict';
  
  // Basic Buffer implementation
  var Buffer = function(arg, encodingOrOffset, length) {
    if (typeof arg === 'number') {
      return new Uint8Array(arg);
    }
    if (typeof arg === 'string') {
      var bytes = [];
      for (var i = 0; i < arg.length; i++) {
        bytes.push(arg.charCodeAt(i) & 0xFF);
      }
      return new Uint8Array(bytes);
    }
    if (arg instanceof ArrayBuffer || arg instanceof Uint8Array) {
      return new Uint8Array(arg);
    }
    if (Array.isArray(arg)) {
      return new Uint8Array(arg);
    }
    return new Uint8Array(0);
  };
  
  Buffer.isBuffer = function(obj) {
    return obj instanceof Uint8Array;
  };
  
  Buffer.from = function(value, encoding) {
    return Buffer(value, encoding);
  };
  
  Buffer.alloc = function(size) {
    return new Uint8Array(size);
  };
  
  Buffer.allocUnsafe = function(size) {
    return new Uint8Array(size);
  };
  
  Buffer.concat = function(list) {
    var length = 0;
    for (var i = 0; i < list.length; i++) {
      length += list[i].length;
    }
    var result = new Uint8Array(length);
    var pos = 0;
    for (var i = 0; i < list.length; i++) {
      result.set(list[i], pos);
      pos += list[i].length;
    }
    return result;
  };
  
  // Make Buffer available globally
  window.Buffer = Buffer;
  globalThis.Buffer = Buffer;
  
  console.log('Buffer polyfill loaded');
})();
