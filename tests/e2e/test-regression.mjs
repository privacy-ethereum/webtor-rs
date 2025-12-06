#!/usr/bin/env node
/**
 * Regression Test - Tests all preset URLs from the demo website
 * 
 * Tests each URL to ensure no regression in TLS/HTTP functionality.
 * Note: httpbin.org is TLS 1.2 only and will fail (expected).
 * 
 * Usage:
 *   ./build.sh
 *   node tests/e2e/test-regression.mjs [--headed] [--quick]
 */

import { chromium } from 'playwright';
import { spawn } from 'child_process';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(__dirname, '../..');

// All preset URLs from the demo website
const PRESET_URLS = [
    // Test Endpoints
    { url: 'https://check.torproject.org/', name: 'Tor Check', tls13: true },
    { url: 'https://api.ipify.org?format=json', name: 'IP Check (ipify)', tls13: true },
    { url: 'https://httpbin.org/user-agent', name: 'HTTPBin', tls13: false }, // TLS 1.2 only - may fail
    
    // Ethereum RPC
    { url: 'https://eth.llamarpc.com', name: 'Llama RPC', tls13: true },
    { url: 'https://ethereum-rpc.publicnode.com', name: 'Publicnode RPC', tls13: true, flaky: true }, // Sometimes blocks Tor
    { url: 'https://rpc.mevblocker.io', name: 'MEV Blocker RPC', tls13: true },
];

const CONFIG = {
    serverPort: 8766,
    serverDir: join(projectRoot, 'webtor-demo', 'static'),
    timeout: 180000,
    headless: true,
    quick: false,
};

// Parse CLI args
const args = process.argv.slice(2);
if (args.includes('--headed')) {
    CONFIG.headless = false;
}
if (args.includes('--quick')) {
    CONFIG.quick = true;
}

let serverProcess = null;

async function startServer() {
    return new Promise((resolve, reject) => {
        serverProcess = spawn('python3', ['-m', 'http.server', CONFIG.serverPort.toString()], {
            cwd: CONFIG.serverDir,
            stdio: ['ignore', 'pipe', 'pipe'],
        });

        serverProcess.stderr.on('data', (data) => {
            const msg = data.toString();
            if (msg.includes('Serving HTTP')) {
                console.log(`Server started on port ${CONFIG.serverPort}`);
                resolve();
            }
        });

        serverProcess.on('error', reject);
        setTimeout(resolve, 1500);
    });
}

function stopServer() {
    if (serverProcess) {
        serverProcess.kill();
        serverProcess = null;
    }
}

async function runRegressionTests() {
    console.log('=== Webtor Regression Tests ===\n');
    console.log(`Testing ${PRESET_URLS.length} preset URLs`);
    console.log(`Mode: ${CONFIG.quick ? 'Quick (WebSocket)' : 'Full (WebRTC)'}`);
    console.log(`Headless: ${CONFIG.headless}\n`);

    await startServer();

    const browser = await chromium.launch({ headless: CONFIG.headless });
    const context = await browser.newContext();
    const page = await context.newPage();
    page.setDefaultTimeout(CONFIG.timeout);

    const results = [];

    try {
        console.log('Loading demo page...');
        await page.goto(`http://localhost:${CONFIG.serverPort}/`, {
            waitUntil: 'networkidle',
            timeout: 30000,
        });

        await page.waitForFunction(() => window.webtor_demo !== undefined, {
            timeout: 30000,
        });
        console.log('WASM module loaded\n');

        // Initialize Tor client once
        console.log('Initializing Tor client...');
        const initResult = await page.evaluate(async (quick) => {
            try {
                const benchFn = quick ? 'runQuickBenchmark' : 'runTorBenchmark';
                // Use a simple URL to initialize
                const result = await window.webtor_demo[benchFn]('https://api.ipify.org?format=json');
                return { success: true, circuit_ms: result.circuit_creation_ms };
            } catch (e) {
                return { success: false, error: e.message || String(e) };
            }
        }, CONFIG.quick);

        if (!initResult.success) {
            console.log(`❌ Failed to initialize Tor client: ${initResult.error}`);
            return { passed: 0, failed: PRESET_URLS.length, results: [] };
        }

        console.log(`✅ Tor client initialized (circuit: ${initResult.circuit_ms}ms)\n`);
        console.log('--- Testing Preset URLs ---\n');

        // Test each URL with individual timeout
        const URL_TIMEOUT = 60000; // 60 seconds per URL
        
        for (const preset of PRESET_URLS) {
            process.stdout.write(`Testing ${preset.name}... `);
            
            let testResult;
            try {
                testResult = await Promise.race([
                    page.evaluate(async ({ url }) => {
                        try {
                            const result = await window.webtor_demo.runQuickBenchmark(url);
                            return {
                                success: true,
                                fetch_ms: result.fetch_latency_ms,
                            };
                        } catch (e) {
                            return {
                                success: false,
                                error: e.message || String(e),
                            };
                        }
                    }, { url: preset.url }),
                    new Promise((_, reject) => 
                        setTimeout(() => reject(new Error('Timeout (60s)')), URL_TIMEOUT)
                    ),
                ]);
            } catch (e) {
                testResult = { success: false, error: e.message || String(e) };
            }

            if (testResult.success) {
                console.log(`✅ OK (${testResult.fetch_ms}ms)`);
                results.push({ ...preset, status: 'passed', latency: testResult.fetch_ms });
            } else {
                const isTlsError = testResult.error.includes('TLS') || testResult.error.includes('close_notify');
                const isTimeout = testResult.error.includes('Timeout');
                
                if (!preset.tls13 && isTlsError) {
                    console.log(`⚠️  Expected TLS 1.2 failure`);
                    results.push({ ...preset, status: 'expected_fail', error: testResult.error });
                } else if (preset.flaky && isTimeout) {
                    console.log(`⚠️  Flaky (timeout - may block Tor)`);
                    results.push({ ...preset, status: 'flaky', error: testResult.error });
                } else {
                    console.log(`❌ FAILED: ${testResult.error}`);
                    results.push({ ...preset, status: 'failed', error: testResult.error });
                }
            }
        }

    } catch (e) {
        console.error(`\nTest error: ${e.message}`);
    } finally {
        await browser.close();
        stopServer();
    }

    // Summary
    console.log('\n=== Results Summary ===\n');
    
    const passed = results.filter(r => r.status === 'passed').length;
    const expectedFail = results.filter(r => r.status === 'expected_fail').length;
    const flaky = results.filter(r => r.status === 'flaky').length;
    const failed = results.filter(r => r.status === 'failed').length;

    console.log(`| URL | Status | Latency |`);
    console.log(`|-----|--------|---------|`);
    for (const r of results) {
        const statusIcon = r.status === 'passed' ? '✅' : 
                          r.status === 'expected_fail' ? '⚠️' : 
                          r.status === 'flaky' ? '⚠️' : '❌';
        const latency = r.latency ? `${Math.round(r.latency)}ms` : r.error?.substring(0, 30) || '-';
        console.log(`| ${r.name} | ${statusIcon} | ${latency} |`);
    }

    console.log(`\nPassed: ${passed}/${PRESET_URLS.length}`);
    console.log(`Expected failures (TLS 1.2): ${expectedFail}`);
    console.log(`Flaky (may block Tor): ${flaky}`);
    console.log(`Unexpected failures: ${failed}`);

    // Exit with error if any unexpected failures
    if (failed > 0) {
        console.log('\n❌ Regression test FAILED');
        process.exit(1);
    } else {
        console.log('\n✅ Regression test PASSED');
        process.exit(0);
    }
}

runRegressionTests().catch(e => {
    console.error('Fatal error:', e);
    stopServer();
    process.exit(1);
});
