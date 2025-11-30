#!/usr/bin/env node
/**
 * Headless browser test for webtor-rs demo
 * 
 * Runs the demo in a headless browser, captures all logs,
 * and reports success/failure automatically.
 * 
 * Usage:
 *   ./build.sh --dev
 *   npm install
 *   node test-headless.mjs
 */

import { chromium } from 'playwright';
import { spawn } from 'child_process';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Configuration
const CONFIG = {
    serverPort: 8765,
    corsProxyPort: 8766,
    serverDir: join(__dirname, 'webtor-demo', 'static'),
    timeout: 120000, // 2 minutes for Tor connection
    headless: true,
    slowMo: 0, // Set to 100 for debugging
};

// Parse CLI args
const args = process.argv.slice(2);
if (args.includes('--headed') || args.includes('-h')) {
    CONFIG.headless = false;
}
if (args.includes('--debug') || args.includes('-d')) {
    CONFIG.slowMo = 100;
}

console.log('🧪 Webtor Headless Test');
console.log('========================');
console.log(`Mode: ${CONFIG.headless ? 'headless' : 'headed'}`);
console.log('');

// Start the CORS proxy
function startCorsProxy() {
    return new Promise((resolve, reject) => {
        const proxy = spawn('node', ['cors-proxy.mjs'], {
            cwd: __dirname,
            stdio: ['ignore', 'pipe', 'pipe'],
        });

        proxy.stdout.on('data', (data) => {
            const output = data.toString();
            console.log(`   [cors-proxy] ${output.trim()}`);
            if (output.includes('CORS proxy running')) {
                resolve(proxy);
            }
        });

        proxy.stderr.on('data', (data) => {
            console.error(`   [cors-proxy error] ${data.toString().trim()}`);
        });

        proxy.on('error', reject);

        // Assume started after 1 second
        setTimeout(() => resolve(proxy), 1000);
    });
}

// Start a simple HTTP server
function startServer() {
    return new Promise((resolve, reject) => {
        const server = spawn('npx', ['serve', '-s', '.', '-p', String(CONFIG.serverPort)], {
            cwd: CONFIG.serverDir,
            stdio: ['ignore', 'pipe', 'pipe'],
        });

        let started = false;
        
        server.stdout.on('data', (data) => {
            const output = data.toString();
            if (output.includes('Accepting connections') || output.includes('Local:')) {
                if (!started) {
                    started = true;
                    console.log(`📦 Server started on port ${CONFIG.serverPort}`);
                    resolve(server);
                }
            }
        });

        server.stderr.on('data', (data) => {
            // Serve outputs to stderr sometimes
            const output = data.toString();
            if (output.includes('Accepting connections') || output.includes('Local:')) {
                if (!started) {
                    started = true;
                    console.log(`📦 Server started on port ${CONFIG.serverPort}`);
                    resolve(server);
                }
            }
        });

        server.on('error', reject);
        
        // Timeout after 10 seconds
        setTimeout(() => {
            if (!started) {
                // Assume it started anyway
                started = true;
                console.log(`📦 Server assumed started on port ${CONFIG.serverPort}`);
                resolve(server);
            }
        }, 3000);
    });
}

async function runTest() {
    let server = null;
    let corsProxy = null;
    let browser = null;
    
    try {
        // Start CORS proxy first
        console.log('🔄 Starting CORS proxy...');
        corsProxy = await startCorsProxy();
        
        // Start server
        server = await startServer();
        
        // Wait a bit for server to be ready
        await new Promise(r => setTimeout(r, 1000));
        
        // Launch browser
        console.log('🌐 Launching browser...');
        browser = await chromium.launch({
            headless: CONFIG.headless,
            slowMo: CONFIG.slowMo,
        });
        
        const context = await browser.newContext();
        const page = await context.newPage();
        
        // Collect all console logs
        const logs = [];
        page.on('console', msg => {
            const text = msg.text();
            const type = msg.type();
            logs.push({ type, text, time: new Date().toISOString() });
            
            // Print important logs
            if (type === 'error') {
                console.log(`❌ [console.error] ${text}`);
            } else if (text.includes('✅') || text.includes('❌') || text.includes('🎉')) {
                console.log(`   ${text}`);
            } else if (text.includes('relays') || text.includes('circuit') || text.includes('ntor') || 
                       text.includes('Creating') || text.includes('Extending') || text.includes('INFO') ||
                       text.includes('WARN') || text.includes('consensus') || text.includes('Channel')) {
                console.log(`   [log] ${text}`);
            }
        });
        
        // Track page errors
        page.on('pageerror', error => {
            console.log(`❌ [page error] ${error.message}`);
            logs.push({ type: 'pageerror', text: error.message, time: new Date().toISOString() });
        });
        
        // Navigate to demo
        const url = `http://localhost:${CONFIG.serverPort}`;
        console.log(`📄 Loading ${url}...`);
        await page.goto(url);
        
        // Wait for WASM to initialize
        console.log('⏳ Waiting for WASM initialization...');
        await page.waitForFunction(() => window.demoApp !== undefined, { timeout: 30000 });
        console.log('✅ WASM initialized');
        
        // Enable debug logging
        console.log('🔧 Enabling debug logs...');
        const debugToggle = await page.$('#debugToggle');
        if (debugToggle) {
            await debugToggle.click();
        }
        
        // Click "Open TorClient" button
        console.log('🔓 Opening TorClient...');
        const openBtn = await page.$('#openBtn');
        await openBtn.click();
        
        // Wait for circuit to be ready (watch the status element)
        console.log('⏳ Waiting for Tor circuit (this may take 30-60 seconds)...');
        
        const startTime = Date.now();
        let circuitReady = false;
        let lastStatus = '';
        
        while (Date.now() - startTime < CONFIG.timeout) {
            // Check status element
            const status = await page.$eval('#status', el => el.textContent);
            
            if (status !== lastStatus) {
                lastStatus = status;
                console.log(`   Status: ${status.replace('Circuit Status:', '').trim()}`);
            }
            
            // Check if circuit is ready
            if (status.includes('ready') || status.includes('Ready')) {
                circuitReady = true;
                break;
            }
            
            // Check for failures
            if (status.includes('failed') || status.includes('Failed') || status.includes('error')) {
                throw new Error(`Circuit failed: ${status}`);
            }
            
            await new Promise(r => setTimeout(r, 1000));
        }
        
        if (!circuitReady) {
            throw new Error('Timeout waiting for circuit');
        }
        
        const connectionTime = ((Date.now() - startTime) / 1000).toFixed(1);
        console.log(`✅ Circuit ready in ${connectionTime}s`);
        
        // Make a test request
        console.log('🌐 Making test request to httpbin.org/ip...');
        const btn1 = await page.$('#btn1');
        await btn1.click();
        
        // Wait for response
        await page.waitForFunction(
            () => {
                const output = document.getElementById('output1');
                return output && (output.textContent.includes('✅') || output.textContent.includes('❌'));
            },
            { timeout: 60000 }
        );
        
        const output1 = await page.$eval('#output1', el => el.textContent);
        console.log(`   Result: ${output1}`);
        
        if (output1.includes('❌')) {
            throw new Error(`Request failed: ${output1}`);
        }
        
        // Get connection logs
        console.log('');
        console.log('📋 Connection logs:');
        const logTextarea = await page.$eval('#output', el => el.value);
        console.log(logTextarea);
        
        console.log('');
        console.log('🎉 TEST PASSED');
        
        return { success: true, logs, connectionTime };
        
    } catch (error) {
        console.log('');
        console.log(`❌ TEST FAILED: ${error.message}`);
        return { success: false, error: error.message, logs: [] };
        
    } finally {
        if (browser) {
            await browser.close();
        }
        if (server) {
            server.kill();
        }
        if (corsProxy) {
            corsProxy.kill();
        }
    }
}

// Run
runTest().then(result => {
    process.exit(result.success ? 0 : 1);
}).catch(err => {
    console.error('Fatal error:', err);
    process.exit(1);
});
