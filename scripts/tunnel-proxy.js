#!/usr/bin/env node

/**
 * Sol Beast Tunnel Bypass Proxy
 * 
 * This proxy server adds the bypass-tunnel-reminder header
 * to all requests, allowing visitors to skip the password screen.
 * 
 * Usage: node tunnel-proxy.js [tunnel-url] [local-port]
 * Example: node tunnel-proxy.js https://solbeast.loca.lt 3000
 */

const http = require('http');
const https = require('https');
const url = require('url');

// Parse command line arguments
const args = process.argv.slice(2);
const TUNNEL_URL = args[0] || 'https://solbeast.loca.lt';
const PROXY_PORT = args[1] || 8888;

// Colors for console output
const colors = {
    reset: '\x1b[0m',
    green: '\x1b[32m',
    yellow: '\x1b[33m',
    blue: '\x1b[34m',
    cyan: '\x1b[36m',
    red: '\x1b[31m'
};

function log(color, message) {
    console.log(`${colors[color]}${message}${colors.reset}`);
}

// Create proxy server
const proxy = http.createServer((clientReq, clientRes) => {
    // Parse the tunnel URL
    const tunnelUrl = url.parse(TUNNEL_URL);
    
    // Prepare proxy request options with bypass headers
    const options = {
        hostname: tunnelUrl.hostname,
        port: tunnelUrl.port || 443,
        path: clientReq.url,
        method: clientReq.method,
        headers: {
            'bypass-tunnel-reminder': 'true',
            'User-Agent': 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 SolBeast-Bypass/1.0',
            'Accept': clientReq.headers.accept || '*/*',
            'Accept-Encoding': 'gzip, deflate, br',
            'Accept-Language': clientReq.headers['accept-language'] || 'en-US,en;q=0.9',
            'Connection': 'keep-alive',
            'Referer': TUNNEL_URL,
            'Host': tunnelUrl.hostname
        }
    };
    
    // Copy specific headers from original request
    if (clientReq.headers['content-type']) {
        options.headers['Content-Type'] = clientReq.headers['content-type'];
    }
    if (clientReq.headers['content-length']) {
        options.headers['Content-Length'] = clientReq.headers['content-length'];
    }
    if (clientReq.headers.cookie) {
        options.headers['Cookie'] = clientReq.headers.cookie;
    }
    
    // Make request to tunnel
    const proxyReq = https.request(options, (proxyRes) => {
        // Check if we got the password page (401)
        if (proxyRes.statusCode === 401) {
            log('yellow', `⚠️  Got 401 - Password page still showing. Retrying with different User-Agent...`);
            
            // Try again with a completely non-standard User-Agent
            options.headers['User-Agent'] = 'SolBeastTunnel/1.0-CustomClient';
            options.headers['X-Bypass-Tunnel'] = 'true';
            
            const retryReq = https.request(options, (retryRes) => {
                // Set CORS headers to allow cross-origin requests
                const headers = {
                    ...retryRes.headers,
                    'Access-Control-Allow-Origin': '*',
                    'Access-Control-Allow-Methods': 'GET, POST, PUT, DELETE, OPTIONS',
                    'Access-Control-Allow-Headers': '*'
                };
                
                clientRes.writeHead(retryRes.statusCode, headers);
                retryRes.pipe(clientRes);
            });
            
            retryReq.on('error', (err) => {
                log('red', `❌ Retry failed: ${err.message}`);
                clientRes.writeHead(502);
                clientRes.end('Bad Gateway - Could not bypass tunnel password page');
            });
            
            clientReq.pipe(retryReq);
            return;
        }
        
        // Set CORS headers to allow cross-origin requests
        const headers = {
            ...proxyRes.headers,
            'Access-Control-Allow-Origin': '*',
            'Access-Control-Allow-Methods': 'GET, POST, PUT, DELETE, OPTIONS',
            'Access-Control-Allow-Headers': '*'
        };
        
        // Forward status and headers
        clientRes.writeHead(proxyRes.statusCode, headers);
        
        // Pipe response
        proxyRes.pipe(clientRes);
    });
    
    // Handle errors
    proxyReq.on('error', (err) => {
        log('red', `❌ Proxy error: ${err.message}`);
        clientRes.writeHead(502);
        clientRes.end('Bad Gateway');
    });
    
    // Pipe request body
    clientReq.pipe(proxyReq);
});

// Start proxy server
proxy.listen(PROXY_PORT, () => {
    log('green', '╔══════════════════════════════════════════════════════════════════╗');
    log('green', '║              Sol Beast Tunnel Bypass Proxy                      ║');
    log('green', '╚══════════════════════════════════════════════════════════════════╝');
    log('blue', '');
    log('cyan', `🔗 Tunnel URL:     ${TUNNEL_URL}`);
    log('cyan', `📍 Proxy URL:      http://localhost:${PROXY_PORT}`);
    log('blue', '');
    log('yellow', '✨ Share this URL instead: http://localhost:' + PROXY_PORT);
    log('yellow', '   No password prompt for visitors!');
    log('blue', '');
    log('red', 'Press Ctrl+C to stop');
    log('blue', '');
});

// Handle graceful shutdown
process.on('SIGINT', () => {
    log('yellow', '\n🛑 Shutting down proxy...');
    proxy.close(() => {
        log('green', '✓ Proxy stopped');
        process.exit(0);
    });
});

process.on('SIGTERM', () => {
    proxy.close(() => {
        process.exit(0);
    });
});
