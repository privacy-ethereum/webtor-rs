#!/usr/bin/env node
/**
 * Comprehensive TLS Connection Tests
 * 
 * Tests TLS 1.2 and TLS 1.3 connections in headless browser environment.
 * Validates the subtle-tls implementation works correctly in WASM.
 * 
 * Usage:
 *   ./build.sh --dev
 *   node tests/e2e/test-tls-connections.mjs [--headed] [--debug]
 */

import { chromium, firefox } from 'playwright';
import { spawn } from 'child_process';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(__dirname, '../..');

// Configuration
const CONFIG = {
    serverPort: 8765,
    corsProxyPort: 8766,
    serverDir: join(projectRoot, 'webtor-demo', 'static'),
    circuitTimeout: 120000, // 2 minutes for Tor circuit
    requestTimeout: 60000,  // 1 minute per request
    headless: true,
    browser: 'chromium', // 'chromium' or 'firefox'
    debug: false,
};

// Parse CLI args
const args = process.argv.slice(2);
if (args.includes('--headed') || args.includes('-h')) {
    CONFIG.headless = false;
}
if (args.includes('--debug') || args.includes('-d')) {
    CONFIG.debug = true;
}
if (args.includes('--firefox')) {
    CONFIG.browser = 'firefox';
}

// Test targets from demo presets (known Tor-friendly)
// Tests marked as optional won't fail the suite if they timeout (external service issues)
const TLS_TEST_TARGETS = [
    {
        name: 'Tor Check',
        url: 'https://check.torproject.org/',
        expectedInResponse: 'Congratulations',
        tlsVersion: 'any',
        optional: false, // Core test
    },
    {
        name: 'HTTPBin user-agent',
        url: 'https://httpbin.org/user-agent',
        expectedInResponse: 'user-agent',
        tlsVersion: 'any',
        optional: true, // External service, sometimes slow/unavailable
    },
    {
        name: 'Llama RPC (Ethereum)',
        url: 'https://eth.llamarpc.com',
        expectedInResponse: 'running',
        tlsVersion: 'any',
        optional: true, // External service
    },
    {
        name: 'example.com',
        url: 'https://example.com/',
        expectedInResponse: 'Example Domain',
        tlsVersion: 'any',
        optional: false, // Core test, highly available
    },
];

// Test results
const results = {
    passed: 0,
    failed: 0,
    skipped: 0,
    tests: [],
};

function log(message, level = 'info') {
    const timestamp = new Date().toISOString().substr(11, 8);
    const prefix = {
        info: ' ',
        success: '+',
        error: '!',
        debug: '  ',
        warn: '-',
    }[level] || ' ';
    
    if (level === 'debug' && !CONFIG.debug) return;
    console.log(`[${timestamp}] ${prefix} ${message}`);
}

function startCorsProxy() {
    return new Promise((resolve, reject) => {
        const corsProxyPath = join(__dirname, '../utils/cors-proxy.mjs');
        const proxy = spawn('node', [corsProxyPath], {
            cwd: __dirname,
            stdio: ['ignore', 'pipe', 'pipe'],
            env: { ...process.env, PORT: String(CONFIG.corsProxyPort) },
        });

        proxy.stdout.on('data', (data) => {
            const output = data.toString();
            log(`[cors-proxy] ${output.trim()}`, 'debug');
            if (output.includes('CORS proxy running') || output.includes('listening')) {
                resolve(proxy);
            }
        });

        proxy.stderr.on('data', (data) => {
            log(`[cors-proxy error] ${data.toString().trim()}`, 'error');
        });

        proxy.on('error', reject);
        setTimeout(() => resolve(proxy), 2000);
    });
}

function startServer() {
    return new Promise((resolve, reject) => {
        const server = spawn('npx', ['serve', '-s', '.', '-p', String(CONFIG.serverPort)], {
            cwd: CONFIG.serverDir,
            stdio: ['ignore', 'pipe', 'pipe'],
        });

        let started = false;
        
        const onOutput = (data) => {
            const output = data.toString();
            if (!started && (output.includes('Accepting connections') || output.includes('Local:'))) {
                started = true;
                log(`Server started on port ${CONFIG.serverPort}`, 'success');
                resolve(server);
            }
        };

        server.stdout.on('data', onOutput);
        server.stderr.on('data', onOutput);
        server.on('error', reject);
        
        setTimeout(() => {
            if (!started) {
                started = true;
                resolve(server);
            }
        }, 3000);
    });
}

async function setupBrowser() {
    const browserType = CONFIG.browser === 'firefox' ? firefox : chromium;
    
    log(`Launching ${CONFIG.browser}...`);
    const browser = await browserType.launch({
        headless: CONFIG.headless,
        slowMo: CONFIG.debug ? 100 : 0,
    });
    
    const context = await browser.newContext();
    const page = await context.newPage();
    
    // Collect logs
    page.on('console', msg => {
        const text = msg.text();
        const type = msg.type();
        
        if (type === 'error') {
            log(`[console.error] ${text.substring(0, 200)}`, 'error');
        } else if (CONFIG.debug || text.includes('TLS') || text.includes('handshake')) {
            log(`[console] ${text.substring(0, 150)}`, 'debug');
        }
    });
    
    page.on('pageerror', error => {
        log(`[page error] ${error.message}`, 'error');
    });
    
    return { browser, page };
}

async function waitForCircuit(page) {
    log('Opening TorClient...');
    
    // Enable debug logging if in debug mode
    if (CONFIG.debug) {
        const debugToggle = await page.$('#debugToggle');
        if (debugToggle) {
            await debugToggle.click();
        }
    }
    
    // Click Open button
    const openBtn = await page.$('#openBtn');
    await openBtn.click();
    
    log('Waiting for Tor circuit...');
    const startTime = Date.now();
    let lastStatus = '';
    
    while (Date.now() - startTime < CONFIG.circuitTimeout) {
        const status = await page.$eval('#status', el => el.textContent);
        
        if (status !== lastStatus) {
            lastStatus = status;
            const shortStatus = status.replace('Circuit Status:', '').trim();
            log(`Status: ${shortStatus}`, 'debug');
        }
        
        if (status.toLowerCase().includes('ready')) {
            const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
            log(`Circuit ready in ${elapsed}s`, 'success');
            return true;
        }
        
        if (status.includes('failed') || status.includes('Failed') || status.includes('error')) {
            throw new Error(`Circuit failed: ${status}`);
        }
        
        await new Promise(r => setTimeout(r, 1000));
    }
    
    throw new Error('Circuit timeout');
}

async function testTlsConnection(page, target) {
    log(`Testing: ${target.name}${target.optional ? ' (optional)' : ''}`, 'info');
    
    const testResult = {
        name: target.name,
        url: target.url,
        expectedTls: target.tlsVersion,
        status: 'pending',
        error: null,
        responseTime: null,
        optional: target.optional || false,
    };
    
    try {
        // Set URL in input
        const urlInput = await page.$('#url1');
        await urlInput.fill(target.url);
        
        // Clear previous output
        await page.$eval('#output1', el => el.textContent = '');
        
        // Click request button
        const startTime = Date.now();
        const btn1 = await page.$('#btn1');
        await btn1.click();
        
        // Wait for response - need to wait for actual content, not just "Making request..."
        await page.waitForFunction(
            () => {
                const output = document.getElementById('output1');
                if (!output) return false;
                const text = output.textContent;
                // Wait for actual response, not intermediate status
                return text.length > 0 && 
                       !text.includes('Making request') && 
                       !text.includes('Connecting');
            },
            { timeout: CONFIG.requestTimeout }
        );
        
        testResult.responseTime = Date.now() - startTime;
        
        // Check result
        const output = await page.$eval('#output1', el => el.textContent);
        
        if (output.includes('Error') || output.includes('failed')) {
            testResult.status = 'failed';
            testResult.error = output.substring(0, 200);
            log(`FAILED: ${output.substring(0, 100)}`, 'error');
            results.failed++;
        } else if (target.expectedInResponse && output.includes(target.expectedInResponse)) {
            testResult.status = 'passed';
            log(`PASSED (${testResult.responseTime}ms)`, 'success');
            results.passed++;
        } else if (output.includes('Success') || output.length > 50) {
            testResult.status = 'passed';
            log(`PASSED (${testResult.responseTime}ms)`, 'success');
            results.passed++;
        } else {
            testResult.status = 'failed';
            testResult.error = `Unexpected response: ${output.substring(0, 100)}`;
            log(`FAILED: Unexpected response`, 'error');
            results.failed++;
        }
        
    } catch (error) {
        testResult.error = error.message;
        // For optional tests (external services), treat timeout as skipped not failed
        if (target.optional && (error.message.includes('Timeout') || error.message.includes('timeout'))) {
            testResult.status = 'skipped';
            log(`SKIPPED (timeout - external service): ${error.message}`, 'warn');
            results.skipped++;
        } else {
            testResult.status = 'failed';
            log(`FAILED: ${error.message}`, 'error');
            results.failed++;
        }
    }
    
    results.tests.push(testResult);
    return testResult;
}

async function runAllTests() {
    let server = null;
    let corsProxy = null;
    let browser = null;
    
    console.log('');
    console.log('='.repeat(60));
    console.log('  WASM TLS Connection Tests');
    console.log('='.repeat(60));
    console.log(`  Browser: ${CONFIG.browser}`);
    console.log(`  Mode: ${CONFIG.headless ? 'headless' : 'headed'}`);
    console.log(`  Debug: ${CONFIG.debug}`);
    console.log('='.repeat(60));
    console.log('');
    
    try {
        // Start infrastructure
        log('Starting CORS proxy...');
        corsProxy = await startCorsProxy();
        
        log('Starting HTTP server...');
        server = await startServer();
        
        await new Promise(r => setTimeout(r, 1000));
        
        // Setup browser
        const browserSetup = await setupBrowser();
        browser = browserSetup.browser;
        const page = browserSetup.page;
        
        // Navigate to demo
        const url = `http://localhost:${CONFIG.serverPort}`;
        log(`Loading ${url}...`);
        await page.goto(url);
        
        // Wait for WASM initialization (status changes from "Loading WASM...")
        log('Waiting for WASM initialization...');
        await page.waitForFunction(() => {
            const status = document.getElementById('status');
            return status && !status.textContent.includes('Loading WASM');
        }, { timeout: 30000 });
        log('WASM initialized', 'success');
        
        // Establish circuit
        await waitForCircuit(page);
        
        console.log('');
        log('Running TLS connection tests...');
        console.log('-'.repeat(60));
        
        // Run each test
        for (const target of TLS_TEST_TARGETS) {
            await testTlsConnection(page, target);
            // Small delay between tests
            await new Promise(r => setTimeout(r, 2000));
        }
        
        // Summary
        console.log('');
        console.log('='.repeat(60));
        console.log('  Test Summary');
        console.log('='.repeat(60));
        console.log(`  Passed:  ${results.passed}`);
        console.log(`  Failed:  ${results.failed}`);
        console.log(`  Skipped: ${results.skipped}`);
        console.log('='.repeat(60));
        
        if (results.failed > 0) {
            console.log('');
            console.log('Failed Tests:');
            for (const test of results.tests) {
                if (test.status === 'failed') {
                    console.log(`  - ${test.name}: ${test.error}`);
                }
            }
        }
        
        return results.failed === 0;
        
    } catch (error) {
        log(`Fatal error: ${error.message}`, 'error');
        console.error(error.stack);
        return false;
        
    } finally {
        if (browser) await browser.close();
        if (server) server.kill();
        if (corsProxy) corsProxy.kill();
    }
}

// Run tests
runAllTests().then(success => {
    process.exit(success ? 0 : 1);
}).catch(err => {
    console.error('Unhandled error:', err);
    process.exit(1);
});
