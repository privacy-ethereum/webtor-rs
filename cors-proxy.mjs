#!/usr/bin/env node
/**
 * Simple CORS proxy for local development/testing
 * 
 * Usage: node cors-proxy.mjs
 * 
 * Then use: http://localhost:8766/?url=<encoded-url>
 */

import http from 'http';
import https from 'https';
import { URL } from 'url';

const PORT = 8766;

const server = http.createServer(async (req, res) => {
    // Add CORS headers to all responses
    res.setHeader('Access-Control-Allow-Origin', '*');
    res.setHeader('Access-Control-Allow-Methods', 'GET, POST, PUT, DELETE, OPTIONS');
    res.setHeader('Access-Control-Allow-Headers', '*');
    
    // Handle preflight
    if (req.method === 'OPTIONS') {
        res.writeHead(204);
        res.end();
        return;
    }
    
    // Parse the request URL
    const requestUrl = new URL(req.url, `http://localhost:${PORT}`);
    const targetUrl = requestUrl.searchParams.get('url');
    
    if (!targetUrl) {
        res.writeHead(400, { 'Content-Type': 'text/plain' });
        res.end('Missing url parameter');
        return;
    }
    
    try {
        const parsedTarget = new URL(targetUrl);
        const protocol = parsedTarget.protocol === 'https:' ? https : http;
        
        console.log(`Proxying: ${targetUrl}`);
        
        const proxyReq = protocol.request(targetUrl, {
            method: req.method,
            headers: {
                'User-Agent': 'Mozilla/5.0 (compatible; webtor-rs/0.1)',
                'Accept': '*/*',
            },
        }, (proxyRes) => {
            // Forward status and content-type
            res.writeHead(proxyRes.statusCode || 200, {
                'Content-Type': proxyRes.headers['content-type'] || 'application/octet-stream',
                'Access-Control-Allow-Origin': '*',
            });
            proxyRes.pipe(res);
        });
        
        proxyReq.on('error', (err) => {
            console.error(`Proxy error: ${err.message}`);
            res.writeHead(502, { 'Content-Type': 'text/plain' });
            res.end(`Proxy error: ${err.message}`);
        });
        
        proxyReq.setTimeout(30000, () => {
            proxyReq.destroy();
            res.writeHead(504, { 'Content-Type': 'text/plain' });
            res.end('Proxy timeout');
        });
        
        proxyReq.end();
        
    } catch (err) {
        console.error(`Error: ${err.message}`);
        res.writeHead(400, { 'Content-Type': 'text/plain' });
        res.end(`Error: ${err.message}`);
    }
});

server.listen(PORT, () => {
    console.log(`🔄 CORS proxy running on http://localhost:${PORT}`);
    console.log(`   Usage: http://localhost:${PORT}/?url=<encoded-url>`);
});
