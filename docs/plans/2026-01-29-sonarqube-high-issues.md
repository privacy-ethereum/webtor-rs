# SonarCloud High-Issue Cleanup Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Eliminate all SonarCloud HIGH issues (CRITICAL + MAJOR) with minimal behavior changes.

**Architecture:** Keep changes localized in existing files. Reduce cognitive complexity by extracting helper functions in the same file and replacing nested control flow with small, single-purpose helpers. Make mechanical shell updates from `[` to `[[` without altering behavior. Adjust accessible names so visible labels are included, and bump the UI version for any UI edits.

**Tech Stack:** JavaScript (Playwright E2E), HTML/CSS/inline JS, Bash, Rust (for verification only).

---

### Task 1: Refactor `tests/e2e/test-webtor-http-through-tor.mjs` (largest complexity)

**Files:**
- Modify: `tests/e2e/test-webtor-http-through-tor.mjs`

**Step 1: Extract page event wiring into helpers**

Add helper functions above `testWebtorHttpThroughTor` (or just below imports):

```js
function attachConsoleLogger(page, consoleMessages) {
    page.on('console', msg => {
        const msgType = msg.type();
        const text = msg.text();
        consoleMessages.push({ type: msgType, text: text, timestamp: new Date().toISOString() });

        if (msgType === 'error' || text.includes('panic') || text.includes('Instant')) {
            console.log('[Browser ' + msgType + '] ' + text);
        }
    });
}

function attachPageErrorLogger(page, pageErrors) {
    page.on('pageerror', error => {
        pageErrors.push({ message: error.message, stack: error.stack, timestamp: new Date().toISOString() });
        console.error('\n' + '!'.repeat(80));
        console.error('PAGE ERROR DETECTED (POTENTIAL WASM PANIC):');
        console.error(error.message);
        console.error('!'.repeat(80) + '\n');
    });
}
```

**Step 2: Extract repeated “Instant panic” detection**

```js
function detectInstantPanic(pageErrors) {
    return pageErrors.find(e => e.message.includes('Instant') || e.message.includes('std::time')) || null;
}
```

**Step 3: Extract navigation + WASM load check**

```js
async function navigateAndVerifyWasm(page, testResults) {
    console.log('[1/8] Navigating to http://localhost:8080...');
    await page.goto('http://localhost:8080', { waitUntil: 'networkidle', timeout: 30000 });
    console.log('      Page loaded successfully\n');

    await page.screenshot({ path: '/tmp/webtor-test-01-initial.png', fullPage: true });
    testResults.screenshots.push('/tmp/webtor-test-01-initial.png');
    console.log('      Screenshot: /tmp/webtor-test-01-initial.png\n');

    console.log('[2/8] Waiting for WASM module initialization...');
    await page.waitForTimeout(3000);

    const wasmLoaded = await page.evaluate(() => typeof window.demoApp !== 'undefined');
    testResults.wasmLoaded = wasmLoaded;
    console.log('      WASM Loaded: ' + (wasmLoaded ? 'YES' : 'NO') + '\n');

    if (!wasmLoaded) {
        throw new Error('WASM module failed to load');
    }

    await page.screenshot({ path: '/tmp/webtor-test-02-wasm-loaded.png', fullPage: true });
    testResults.screenshots.push('/tmp/webtor-test-02-wasm-loaded.png');
}
```

**Step 4: Extract “open TorClient and wait for circuit”**

```js
async function openTorClientAndWait(page, pageErrors, testResults) {
    console.log('[3/8] Opening TorClient connection...');
    const openBtn = await page.$('#openBtn');
    if (!openBtn) {
        throw new Error('Open button not found');
    }

    await openBtn.click();
    console.log('      Clicked "Open TorClient" button');
    testResults.torClientOpened = true;

    console.log('      Waiting for Tor circuit establishment (this may take 30-90 seconds)...\n');

    let circuitReady = false;
    let waitTime = 0;
    const maxWaitTime = 120000;
    const checkInterval = 5000;

    while (!circuitReady && waitTime < maxWaitTime) {
        await page.waitForTimeout(checkInterval);
        waitTime += checkInterval;

        const statusText = await page.$eval('#status', el => el.textContent);
        console.log('      [' + (waitTime / 1000) + 's] Status: ' + statusText.trim());

        if (statusText.includes('Circuit ready') || statusText.includes('Connected')) {
            circuitReady = true;
            testResults.circuitEstablished = true;
            console.log('      Circuit established!\n');
        }

        const instantError = detectInstantPanic(pageErrors);
        if (instantError) {
            testResults.instantPanicDetected = true;
            throw new Error('WASM panic detected: std::time::Instant issue NOT FIXED');
        }
    }

    if (!circuitReady) {
        console.log('      Warning: Circuit did not establish within timeout, proceeding anyway...\n');
    }

    await page.screenshot({ path: '/tmp/webtor-test-03-circuit-status.png', fullPage: true });
    testResults.screenshots.push('/tmp/webtor-test-03-circuit-status.png');
}
```

**Step 5: Extract HTTP request + response loop**

```js
async function runHttpRequest(page, pageErrors, testResults) {
    console.log('[4/8] Preparing to make HTTP request through Tor...');
    const url1Input = await page.$('#url1');
    const btn1 = await page.$('#btn1');

    if (!url1Input || !btn1) {
        throw new Error('Request button or URL input not found');
    }

    await url1Input.click({ clickCount: 3 });
    await url1Input.fill('https://httpbin.org/ip');
    console.log('      Target URL: https://httpbin.org/ip');

    await page.screenshot({ path: '/tmp/webtor-test-04-before-request.png', fullPage: true });
    testResults.screenshots.push('/tmp/webtor-test-04-before-request.png');

    console.log('[5/8] Clicking "Make Request 1" button...');
    await btn1.click();
    console.log('      Request initiated\n');

    console.log('[6/8] Waiting for HTTP response (max 60 seconds)...');

    let responseReceived = false;
    let requestWaitTime = 0;
    const maxRequestWaitTime = 60000;

    while (!responseReceived && requestWaitTime < maxRequestWaitTime) {
        await page.waitForTimeout(2000);
        requestWaitTime += 2000;

        const output1 = await page.$eval('#output1', el => el.textContent);

        if (requestWaitTime % 10000 === 0) {
            console.log('      [' + (requestWaitTime / 1000) + 's] Waiting for response...');
            console.log('      Current output: ' + output1.substring(0, 100) + '...');
        }

        if (output1.includes('"origin"') || output1.includes('origin') || output1.includes('error') || output1.includes('Error')) {
            responseReceived = true;
            testResults.responseReceived = true;

            if (output1.includes('"origin"') || (output1.includes('origin') && !output1.includes('error'))) {
                testResults.httpRequestSuccess = true;
                console.log('      HTTP Response received!');
                console.log('      Response: ' + output1.substring(0, 200) + '\n');
            } else {
                testResults.httpRequestError = output1;
                console.log('      Request completed with error:');
                console.log('      ' + output1 + '\n');
            }
        }

        const instantError = detectInstantPanic(pageErrors);
        if (instantError) {
            testResults.instantPanicDetected = true;
            testResults.httpRequestError = instantError.message;
            throw new Error('WASM panic during HTTP request: std::time::Instant issue NOT FIXED');
        }
    }

    await page.screenshot({ path: '/tmp/webtor-test-05-after-request.png', fullPage: true });
    testResults.screenshots.push('/tmp/webtor-test-05-after-request.png');
}
```

**Step 6: Extract final log capture + summary + report**

```js
async function writeFinalArtifacts(page, testResults, pageErrors, consoleMessages) {
    console.log('[7/8] Checking connection logs...');
    const logsTextarea = await page.$('#output');
    if (logsTextarea) {
        const logs = await logsTextarea.inputValue();
        writeFileSync('/tmp/webtor-test-connection-logs.txt', logs);
        console.log('      Connection logs saved to /tmp/webtor-test-connection-logs.txt');
        console.log('      Log size: ' + logs.length + ' characters\n');
    }

    console.log('[8/8] Final status check...');
    const finalStatus = await page.$eval('#status', el => el.textContent);
    console.log('      Final circuit status: ' + finalStatus.trim() + '\n');

    await page.screenshot({ path: '/tmp/webtor-test-06-final.png', fullPage: true });
    testResults.screenshots.push('/tmp/webtor-test-06-final.png');

    console.log('\n' + '='.repeat(80));
    console.log('TEST RESULTS SUMMARY');
    console.log('='.repeat(80));
    console.log('');
    console.log('WASM Module Loaded:           ' + (testResults.wasmLoaded ? 'PASS' : 'FAIL'));
    console.log('TorClient Opened:             ' + (testResults.torClientOpened ? 'PASS' : 'FAIL'));
    console.log('Tor Circuit Established:      ' + (testResults.circuitEstablished ? 'PASS' : 'WARN'));
    console.log('HTTP Request Completed:       ' + (testResults.responseReceived ? 'PASS' : 'FAIL'));
    console.log('HTTP Request Success:         ' + (testResults.httpRequestSuccess ? 'PASS' : 'FAIL'));
    console.log('std::time::Instant Panic:     ' + (testResults.instantPanicDetected ? 'DETECTED' : 'NOT DETECTED'));
    console.log('');

    if (testResults.httpRequestError) {
        console.log('HTTP Request Error Details:');
        console.log(testResults.httpRequestError);
        console.log('');
    }

    console.log('Page Errors Detected:         ' + pageErrors.length);
    if (pageErrors.length > 0) {
        console.log('\nPage Errors:');
        pageErrors.forEach((err, i) => {
            console.log('  ' + (i + 1) + '. ' + err.message);
        });
    }
    console.log('');

    console.log('Screenshots Generated:        ' + testResults.screenshots.length);
    testResults.screenshots.forEach(path => {
        console.log('  - ' + path);
    });
    console.log('');

    const reportPath = '/tmp/webtor-test-results.json';
    writeFileSync(reportPath, JSON.stringify({
        testResults,
        pageErrors,
        timestamp: new Date().toISOString(),
        consoleMessageCount: consoleMessages.length,
    }, null, 2));
    console.log('Detailed results: ' + reportPath);
    console.log('Console messages: /tmp/webtor-test-console-messages.json');
    writeFileSync('/tmp/webtor-test-console-messages.json', JSON.stringify(consoleMessages, null, 2));
    console.log('');

    console.log('='.repeat(80));
    console.log('FINAL VERDICT');
    console.log('='.repeat(80));

    if (testResults.instantPanicDetected) {
        console.log('FAIL: std::time::Instant WASM panic STILL PRESENT');
        console.log('The fix did NOT resolve the issue.');
    } else if (testResults.httpRequestSuccess) {
        console.log('PASS: HTTP request through Tor completed successfully!');
        console.log('The std::time::Instant fix appears to be working correctly.');
    } else if (testResults.responseReceived) {
        console.log('PARTIAL: HTTP request completed but with errors');
        console.log('No std::time::Instant panic detected, which is good.');
    } else {
        console.log('INCONCLUSIVE: Could not complete HTTP request test');
        console.log('No std::time::Instant panic detected during connection phase.');
    }
    console.log('='.repeat(80));
    console.log('');
}
```

**Step 7: Update `testWebtorHttpThroughTor` to call helpers**

```js
async function testWebtorHttpThroughTor() {
    console.log('='.repeat(80));
    console.log('WEBTOR-RS HTTP THROUGH TOR - FUNCTIONAL BROWSER TEST');
    console.log('Testing fix for std::time::Instant WASM panic issue');
    console.log('='.repeat(80));
    console.log('');

    const browser = await chromium.launch({ headless: false, args: ['--disable-web-security'], slowMo: 100 });
    const context = await browser.newContext({ viewport: { width: 1920, height: 1080 } });
    const page = await context.newPage();

    const consoleMessages = [];
    const pageErrors = [];
    attachConsoleLogger(page, consoleMessages);
    attachPageErrorLogger(page, pageErrors);

    const testResults = {
        wasmLoaded: false,
        torClientOpened: false,
        circuitEstablished: false,
        httpRequestSuccess: false,
        httpRequestError: null,
        responseReceived: false,
        instantPanicDetected: false,
        screenshots: [],
    };

    try {
        await navigateAndVerifyWasm(page, testResults);
        await openTorClientAndWait(page, pageErrors, testResults);
        await runHttpRequest(page, pageErrors, testResults);
        await writeFinalArtifacts(page, testResults, pageErrors, consoleMessages);
    } catch (error) {
        console.error('\n' + '!'.repeat(80));
        console.error('TEST EXCEPTION:');
        console.error(error.message);
        console.error('!'.repeat(80) + '\n');

        try {
            await page.screenshot({ path: '/tmp/webtor-test-ERROR.png', fullPage: true });
            testResults.screenshots.push('/tmp/webtor-test-ERROR.png');
            console.log('Error screenshot saved to /tmp/webtor-test-ERROR.png\n');
        } catch (e) {
            console.error('Failed to capture error screenshot');
        }
    } finally {
        await browser.close();
        console.log('Browser closed.');
        console.log('Test complete.\n');
    }
}
```

**Step 8: Manual verification**

Run (requires demo server on `http://localhost:8080`):

```bash
node tests/e2e/test-webtor-http-through-tor.mjs
```

Expected: script runs; no exceptions from refactor (behavior unchanged).

**Step 9: Commit**

```bash
git add tests/e2e/test-webtor-http-through-tor.mjs
git commit -m "refactor: split webtor-http-through-tor e2e flow"
```

---

### Task 2: Refactor `tests/e2e/test-regression.mjs`

**Files:**
- Modify: `tests/e2e/test-regression.mjs`

**Step 1: Extract CLI parsing + config logging**

```js
function applyCliFlags(config, args) {
    if (args.includes('--headed')) {
        config.headless = false;
    }
    if (args.includes('--quick')) {
        config.quick = true;
    }
}

function logRunHeader() {
    console.log('=== Webtor Regression Tests ===\n');
    console.log(`Testing ${PRESET_URLS.length} preset URLs`);
    console.log(`Mode: ${CONFIG.quick ? 'Quick (WebSocket)' : 'Full (WebRTC)'}`);
    console.log(`Headless: ${CONFIG.headless}\n`);
}
```

**Step 2: Extract page startup and Tor init**

```js
async function launchPage() {
    const browser = await chromium.launch({ headless: CONFIG.headless });
    const context = await browser.newContext();
    const page = await context.newPage();
    page.setDefaultTimeout(CONFIG.timeout);
    return { browser, page };
}

async function loadDemo(page) {
    console.log('Loading demo page...');
    await page.goto(`http://localhost:${CONFIG.serverPort}/`, { waitUntil: 'networkidle', timeout: 30000 });
    await page.waitForFunction(() => window.webtor_demo !== undefined, { timeout: 30000 });
    console.log('WASM module loaded\n');
}

async function initTorClient(page) {
    console.log('Initializing Tor client...');
    const initResult = await page.evaluate(async (quick) => {
        try {
            const benchFn = quick ? 'runQuickBenchmark' : 'runTorBenchmark';
            const result = await window.webtor_demo[benchFn]('https://api.ipify.org?format=json');
            return { success: true, circuit_ms: result.circuit_creation_ms };
        } catch (e) {
            return { success: false, error: e.message || String(e) };
        }
    }, CONFIG.quick);

    if (!initResult.success) {
        console.log(`Failed to initialize Tor client: ${initResult.error}`);
        return { ok: false, results: [] };
    }

    console.log(`Tor client initialized (circuit: ${initResult.circuit_ms}ms)\n`);
    console.log('--- Testing Preset URLs ---\n');
    return { ok: true };
}
```

**Step 3: Extract URL test runner + classification**

```js
async function runUrlTest(page, preset) {
    const URL_TIMEOUT = 60000;

    try {
        const testResult = await Promise.race([
            page.evaluate(async ({ url }) => {
                try {
                    const result = await window.webtor_demo.runQuickBenchmark(url);
                    return { success: true, fetch_ms: result.fetch_latency_ms };
                } catch (e) {
                    return { success: false, error: e.message || String(e) };
                }
            }, { url: preset.url }),
            new Promise((_, reject) => setTimeout(() => reject(new Error('Timeout (60s)')), URL_TIMEOUT)),
        ]);
        return testResult;
    } catch (e) {
        return { success: false, error: e.message || String(e) };
    }
}

function classifyTestResult(preset, testResult) {
    if (testResult.success) {
        return { ...preset, status: 'passed', latency: testResult.fetch_ms };
    }

    const isTlsError = testResult.error.includes('TLS') || testResult.error.includes('close_notify');
    const isTimeout = testResult.error.includes('Timeout');

    if (!preset.tls13 && isTlsError) {
        return { ...preset, status: 'expected_fail', error: testResult.error };
    }
    if (preset.flaky && isTimeout) {
        return { ...preset, status: 'flaky', error: testResult.error };
    }
    return { ...preset, status: 'failed', error: testResult.error };
}
```

**Step 4: Extract summary printing + exit**

```js
function printSummary(results) {
    console.log('\n=== Results Summary ===\n');

    const passed = results.filter(r => r.status === 'passed').length;
    const expectedFail = results.filter(r => r.status === 'expected_fail').length;
    const flaky = results.filter(r => r.status === 'flaky').length;
    const failed = results.filter(r => r.status === 'failed').length;

    console.log('| URL | Status | Latency |');
    console.log('|-----|--------|---------|');
    for (const r of results) {
    const statusIcon = r.status === 'passed' ? '\u2705' : r.status === 'expected_fail' ? '\u26A0' : r.status === 'flaky' ? '\u26A0' : '\u274C';
        const latency = r.latency ? `${Math.round(r.latency)}ms` : r.error?.substring(0, 30) || '-';
        console.log(`| ${r.name} | ${statusIcon} | ${latency} |`);
    }

    console.log(`\nPassed: ${passed}/${PRESET_URLS.length}`);
    console.log(`Expected failures (TLS 1.2): ${expectedFail}`);
    console.log(`Flaky (may block Tor): ${flaky}`);
    console.log(`Unexpected failures: ${failed}`);

    if (failed > 0) {
        console.log('\nRegression test FAILED');
        process.exit(1);
    }

    console.log('\nRegression test PASSED');
    process.exit(0);
}
```

**Step 5: Update `runRegressionTests` to use helpers**

Refactor the body to:

```js
async function runRegressionTests() {
    logRunHeader();
    await startServer();

    const { browser, page } = await launchPage();
    const results = [];

    try {
        await loadDemo(page);
        const init = await initTorClient(page);
        if (!init.ok) {
            return { passed: 0, failed: PRESET_URLS.length, results: [] };
        }

        for (const preset of PRESET_URLS) {
            process.stdout.write(`Testing ${preset.name}... `);
            const testResult = await runUrlTest(page, preset);
            const classified = classifyTestResult(preset, testResult);
            results.push(classified);

            if (classified.status === 'passed') {
                console.log(`OK (${classified.latency}ms)`);
            } else if (classified.status === 'expected_fail') {
                console.log('Expected TLS 1.2 failure');
            } else if (classified.status === 'flaky') {
                console.log('Flaky (timeout - may block Tor)');
            } else {
                console.log(`FAILED: ${classified.error}`);
            }
        }
    } catch (e) {
        console.error(`\nTest error: ${e.message}`);
    } finally {
        await browser.close();
        stopServer();
    }

    printSummary(results);
}
```

**Step 6: Manual verification**

Run (requires demo server and network):

```bash
node tests/e2e/test-regression.mjs --quick
```

Expected: same behavior and exit code as before refactor.

**Step 7: Commit**

```bash
git add tests/e2e/test-regression.mjs
git commit -m "refactor: split regression e2e flow"
```

---

### Task 3: Refactor `tests/e2e/test-demo-playwright.mjs`

**Files:**
- Modify: `tests/e2e/test-demo-playwright.mjs`

**Step 1: Extract setup + event wiring**

```js
function attachConsoleLogger(page, consoleMessages) {
    page.on('console', msg => {
        const msgType = msg.type();
        const text = msg.text();
        consoleMessages.push({ type: msgType, text: text });
        console.log('[Browser ' + msgType + '] ' + text);
    });
}

function attachPageErrorLogger(page, pageErrors) {
    page.on('pageerror', error => {
        pageErrors.push(error.message);
        console.error('[Page Error] ' + error.message);
    });
}

async function launchPage() {
    const browser = await chromium.launch({ headless: true, args: ['--disable-web-security'] });
    const context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
    const page = await context.newPage();
    return { browser, page };
}
```

**Step 2: Extract UI checks + optional click**

```js
async function logUiElements(page) {
    console.log('5. Checking for UI elements...');
    const openBtn = await page.$('#openBtn');
    const closeBtn = await page.$('#closeBtn');
    const snowflakeUrl = await page.$('#snowflakeUrl');
    const status = await page.$('#status');

    console.log('   - Open Button: ' + (openBtn ? 'Found' : 'NOT FOUND'));
    console.log('   - Close Button: ' + (closeBtn ? 'Found' : 'NOT FOUND'));
    console.log('   - Snowflake URL Input: ' + (snowflakeUrl ? 'Found' : 'NOT FOUND'));
    console.log('   - Status Display: ' + (status ? 'Found' : 'NOT FOUND'));

    if (status) {
        const statusText = await status.textContent();
        console.log('   - Initial Status: "' + statusText + '"');
    }

    return { openBtn, status };
}

async function maybeClickOpen(page, openBtn, status) {
    if (!openBtn) {
        console.log('8. SKIPPING button click - WASM not loaded or button not found');
        return;
    }

    console.log('8. Attempting to click "Open TorClient" button...');
    await openBtn.click();
    console.log('   Button clicked, waiting for response...');

    await page.waitForTimeout(5000);
    const newStatusText = await status.textContent();
    console.log('   - Status after click: "' + newStatusText + '"');

    console.log('9. Taking post-click screenshot...');
    await page.screenshot({ path: '/tmp/webtor-demo-clicked.png', fullPage: true });
    console.log('   Screenshot saved to /tmp/webtor-demo-clicked.png');

    const logsTextarea = await page.$('#output');
    if (logsTextarea) {
        const logs = await logsTextarea.inputValue();
        console.log('10. Connection logs:');
        if (logs.length > 0) {
            console.log(logs.substring(0, 500) + (logs.length > 500 ? '...' : ''));
        } else {
            console.log('   (No logs yet)');
        }
    }
}
```

**Step 3: Extract summary printing**

```js
function logSummary(consoleMessages, pageErrors, wasmLoaded) {
    console.log('\n=== TEST SUMMARY ===');
    console.log('Console Messages: ' + consoleMessages.length);
    console.log('Page Errors: ' + pageErrors.length);

    if (pageErrors.length > 0) {
        console.log('\nPage Errors Detected:');
        pageErrors.forEach((err, i) => {
            console.log('  ' + (i + 1) + '. ' + err);
        });
    }

    console.log('\nConsole Message Breakdown:');
    const messageTypes = consoleMessages.reduce((acc, msg) => {
        acc[msg.type] = (acc[msg.type] || 0) + 1;
        return acc;
    }, {});
    Object.entries(messageTypes).forEach(([type, count]) => {
        console.log('  - ' + type + ': ' + count);
    });

    writeFileSync('/tmp/webtor-demo-console.json', JSON.stringify(consoleMessages, null, 2));
    console.log('\nDetailed console logs saved to /tmp/webtor-demo-console.json');

    console.log('\n=== TEST STATUS ===');
    if (pageErrors.length === 0 && wasmLoaded) {
        console.log('PASS: WASM loaded successfully, no critical errors');
    } else if (pageErrors.length === 0) {
        console.log('PARTIAL: No errors, but WASM may not be fully initialized');
    } else {
        console.log('FAIL: Errors detected during execution');
    }
}
```

**Step 4: Update `testWebtorDemo` to use helpers**

Refactor the main function to:

```js
async function testWebtorDemo() {
    console.log('Starting Webtor-rs WASM Demo Test...\n');

    const { browser, page } = await launchPage();
    const consoleMessages = [];
    const pageErrors = [];
    attachConsoleLogger(page, consoleMessages);
    attachPageErrorLogger(page, pageErrors);

    try {
        console.log('1. Navigating to http://localhost:8000...');
        await page.goto('http://localhost:8000', { waitUntil: 'networkidle', timeout: 30000 });

        console.log('2. Taking initial screenshot...');
        await page.screenshot({ path: '/tmp/webtor-demo-initial.png', fullPage: true });
        console.log('   Screenshot saved to /tmp/webtor-demo-initial.png');

        console.log('3. Waiting for WASM module to load...');
        await page.waitForTimeout(3000);

        console.log('4. Checking page title...');
        const title = await page.title();
        console.log('   Page title: "' + title + '"');

        const { openBtn, status } = await logUiElements(page);

        console.log('6. Checking for WASM initialization...');
        const wasmLoaded = await page.evaluate(() => typeof window.demoApp !== 'undefined');
        console.log('   - WASM App Object: ' + (wasmLoaded ? 'Initialized' : 'NOT FOUND'));

        console.log('7. Taking post-load screenshot...');
        await page.screenshot({ path: '/tmp/webtor-demo-loaded.png', fullPage: true });
        console.log('   Screenshot saved to /tmp/webtor-demo-loaded.png');

        if (wasmLoaded && openBtn) {
            await maybeClickOpen(page, openBtn, status);
        } else {
            console.log('8. SKIPPING button click - WASM not loaded or button not found');
        }

        logSummary(consoleMessages, pageErrors, wasmLoaded);
    } catch (error) {
        console.error('\nTEST FAILED with exception: ' + error.message);
        console.error(error.stack);

        try {
            await page.screenshot({ path: '/tmp/webtor-demo-error.png', fullPage: true });
            console.log('Error screenshot saved to /tmp/webtor-demo-error.png');
        } catch (e) {
            console.error('Failed to take error screenshot: ' + e.message);
        }
    } finally {
        await browser.close();
        console.log('\nBrowser closed.');
    }
}
```

**Step 5: Manual verification**

```bash
node tests/e2e/test-demo-playwright.mjs
```

Expected: same console output and screenshots as before.

**Step 6: Commit**

```bash
git add tests/e2e/test-demo-playwright.mjs
git commit -m "refactor: split demo playwright e2e flow"
```

---

### Task 4: Refactor `tests/e2e/test-example.mjs`

**Files:**
- Modify: `tests/e2e/test-example.mjs`

**Step 1: Extract browser startup + console wiring**

```js
function attachConsoleLogger(page) {
    page.on('console', msg => {
        const text = msg.text();
        const type = msg.type();
        if (type === 'error') {
            console.log(` [error] ${text}`);
        } else if (text.includes('INFO') || text.includes('WARN') || text.includes('ERROR') ||
                   text.includes('circuit') || text.includes('Channel') || text.includes('consensus') ||
                   text.includes('WebTunnel') || text.includes('')) {
            console.log(`   [log] ${text.substring(0, 200)}`);
        }
    });

    page.on('pageerror', error => {
        console.log(` [page error] ${error.message}`);
    });
}

async function launchExamplePage() {
    const browser = await chromium.launch({ headless: true });
    const context = await browser.newContext();
    const page = await context.newPage();
    return { browser, page };
}
```

**Step 2: Extract connection wait loop**

```js
async function waitForTorConnection(page) {
    const startTime = Date.now();
    let connected = false;
    let lastStatus = '';

    while (Date.now() - startTime < CONFIG.timeout) {
        try {
            const badge = await page.$('span.chakra-badge');
            if (badge) {
                const status = await badge.textContent();
                if (status !== lastStatus) {
                    lastStatus = status;
                    console.log(`   Status: ${status}`);
                }

                if (status.includes('Connected')) {
                    connected = true;
                    break;
                }

                if (status.includes('failed') || status.includes('Failed')) {
                    throw new Error(`Connection failed: ${status}`);
                }
            }
        } catch (e) {
            // Ignore selector errors
        }

        await new Promise(r => setTimeout(r, 2000));
    }

    return { connected, elapsedMs: Date.now() - startTime };
}
```

**Step 3: Update `runTest` to use helpers**

Refactor the function so it:

- Uses `launchExamplePage()`
- Wires logging with `attachConsoleLogger(page)`
- Calls `waitForTorConnection(page)` and handles timeout
- Keeps the same click and assertion logic

**Step 4: Manual verification**

```bash
node tests/e2e/test-example.mjs
```

Expected: same exit code and logs as before.

**Step 5: Commit**

```bash
git add tests/e2e/test-example.mjs
git commit -m "refactor: split example e2e flow"
```

---

### Task 5: Refactor `tests/e2e/test-http-flow.mjs`

**Files:**
- Modify: `tests/e2e/test-http-flow.mjs`

**Step 1: Extract event wiring and critical issue analysis**

```js
function attachConsoleLogger(page, consoleMessages) {
  page.on('console', msg => {
    const msgType = msg.type();
    const text = msg.text();
    consoleMessages.push({ type: msgType, text });
    console.log('[' + msgType + '] ' + text);
  });
}

function attachPageErrorLogger(page, pageErrors) {
  page.on('pageerror', error => {
    pageErrors.push(error.message);
    console.error('Page error:', error.message);
  });
}

function analyzeIssues(consoleMessages, pageErrors) {
  const instantErrors = consoleMessages.filter(m =>
    m.text.includes('std::time::Instant') ||
    m.text.includes('Instant not available')
  );

  const wasmPanics = consoleMessages.filter(m =>
    m.text.includes('panicked at') ||
    m.text.includes('panic')
  );

  return { instantErrors, wasmPanics, pageErrors };
}
```

**Step 2: Extract UI lookup helpers**

```js
async function findFirstMatchingLocator(page, selectors, label) {
  for (const selector of selectors) {
    const element = page.locator(selector).first();
    if (await element.count() > 0) {
      console.log('Found ' + label + ' using selector: ' + selector);
      return element;
    }
  }
  return null;
}
```

**Step 3: Update main flow to use helpers**

- Use `findFirstMatchingLocator` for input/button selection.
- Use `analyzeIssues` to print summary blocks.
- Keep the exact same logging and screenshots.

**Step 4: Manual verification**

```bash
node tests/e2e/test-http-flow.mjs
```

Expected: same output and screenshots as before.

**Step 5: Commit**

```bash
git add tests/e2e/test-http-flow.mjs
git commit -m "refactor: split http-flow e2e analysis"
```

---

### Task 6: Refactor `tests/e2e/test-tor-http.mjs`

**Files:**
- Modify: `tests/e2e/test-tor-http.mjs`

**Step 1: Extract analysis helpers**

```js
function classifyConsoleMessages(consoleMessages) {
  const instantErrors = consoleMessages.filter(m =>
    m.text.includes('std::time::Instant') ||
    m.text.includes('Instant not available') ||
    m.text.includes('instant')
  );

  const wasmPanics = consoleMessages.filter(m =>
    (m.text.includes('panicked at') || m.text.includes('panic')) &&
    !m.text.includes('no panic')
  );

  const httpSuccess = consoleMessages.filter(m =>
    m.text.includes('HTTP request success') ||
    m.text.includes('Response received') ||
    m.text.includes('200 OK')
  );

  const httpErrors = consoleMessages.filter(m =>
    m.text.includes('HTTP request failed') ||
    m.text.includes('Request error') ||
    m.text.includes('Error making request')
  );

  return { instantErrors, wasmPanics, httpSuccess, httpErrors };
}
```

**Step 2: Extract circuit wait loop**

```js
async function waitForCircuit(page) {
  let circuitReady = false;
  for (let i = 0; i < 24; i++) {
    const statusText = await page.locator('#status').textContent();
    console.log('Status check ' + (i + 1) + ': ' + statusText.substring(0, 100));

    if (statusText.includes('Ready') || statusText.includes('Established') || statusText.includes('circuit created')) {
      console.log('CIRCUIT READY!');
      circuitReady = true;
      break;
    }

    await page.waitForTimeout(5000);
  }

  return circuitReady;
}
```

**Step 3: Update main flow to use helpers**

- Use `classifyConsoleMessages` for analysis output.
- Use `waitForCircuit` for circuit readiness.
- Keep screenshots and log output unchanged.

**Step 4: Manual verification**

```bash
node tests/e2e/test-tor-http.mjs
```

Expected: same output and screenshots as before.

**Step 5: Commit**

```bash
git add tests/e2e/test-tor-http.mjs
git commit -m "refactor: split tor http e2e analysis"
```

---

### Task 7: Refactor `webtor-demo/static/index.html` (complexity + a11y + version bump)

**Files:**
- Modify: `webtor-demo/static/index.html`

**Step 1: Adjust accessible names to include visible labels**

Update these buttons so the accessible name matches the visible label (remove mismatched `aria-label` and use `title` if extra context is still needed):

```html
<button id="openBtn">Open (WebSocket)</button>
<button id="openWebRtcBtn">Open (WebRTC)</button>
<button id="updateBtn" class="secondary" disabled>Refresh Circuit</button>
<button id="btn1" disabled>Send Request</button>
<button id="btnIsolated" disabled>Send Isolated</button>
```

If additional context is required, add a `title` attribute matching the prior description (no change to accessible name).

**Step 2: Split `updateCircuitDisplay` into helpers**

Introduce helpers near the function (same script block):

```js
function clearCircuitDisplay(circuitDisplay) {
    circuitDisplay.classList.remove('visible');
}

function createArrowNode() {
    const arrow = document.createElement('div');
    arrow.className = 'circuit-arrow';
    arrow.textContent = '\u2192';
    arrow.setAttribute('aria-hidden', 'true');
    return arrow;
}

function appendYouNode(circuitPath) {
    const youNode = document.createElement('div');
    youNode.className = 'circuit-relay circuit-you';
    youNode.innerHTML = `
        <div class="circuit-relay-role">You</div>
        <div class="circuit-relay-nickname">Browser</div>
        <div class="circuit-relay-address">local</div>
    `;
    circuitPath.appendChild(youNode);
    circuitPath.appendChild(createArrowNode());
}

function normalizeRelayAddress(relay) {
    if (relay.role === 'Bridge' && (relay.address === 'Snowflake (WebRTC)' || relay.address === '0.0.0.0')) {
        return 'via WebRTC';
    }
    return relay.address;
}

function appendRelayNodes(relays, circuitPath) {
    relays.forEach((relay) => {
        const node = document.createElement('div');
        node.className = 'circuit-relay';

        const displayAddress = normalizeRelayAddress(relay);
        node.innerHTML = `
            <div class="circuit-relay-role">${escapeHtml(relay.role)}</div>
            <div class="circuit-relay-nickname">${escapeHtml(relay.nickname)}</div>
            <div class="circuit-relay-address">${escapeHtml(displayAddress)}</div>
        `;
        circuitPath.appendChild(node);
        circuitPath.appendChild(createArrowNode());
    });
}

function resolveTargetInfo(targetUrl) {
    let targetHost = 'Internet';
    let targetAddress = '';
    if (targetUrl) {
        try {
            const url = new URL(targetUrl);
            targetHost = url.hostname;
            targetAddress = url.port ? `:${url.port}` : (url.protocol === 'https:' ? ':443' : ':80');
        } catch (e) {
            targetHost = targetUrl;
        }
    }
    return { targetHost, targetAddress };
}

function appendTargetNode(circuitPath, targetUrl) {
    const targetNode = document.createElement('div');
    targetNode.className = 'circuit-relay circuit-target';
    const { targetHost, targetAddress } = resolveTargetInfo(targetUrl);
    targetNode.innerHTML = `
        <div class="circuit-relay-role">Target</div>
        <div class="circuit-relay-nickname">${escapeHtml(targetHost)}</div>
        <div class="circuit-relay-address">${escapeHtml(targetAddress)}</div>
    `;
    circuitPath.appendChild(targetNode);
}
```

Refactor `updateCircuitDisplay` to:

```js
async function updateCircuitDisplay(appInstance, targetUrl = null) {
    if (!appInstance) {
        clearCircuitDisplay(circuitDisplay);
        return;
    }

    try {
        const relays = await appInstance.getCircuitRelays();
        if (!relays || relays.length === 0) {
            clearCircuitDisplay(circuitDisplay);
            return;
        }

        circuitPath.innerHTML = '';
        appendYouNode(circuitPath);
        appendRelayNodes(relays, circuitPath);
        appendTargetNode(circuitPath, targetUrl);
        circuitDisplay.classList.add('visible');

    const relayNames = relays.map(r => r.nickname).join(' \u2192 ');
        logCallback('INFO', 'circuit', `Circuit: ${relayNames}`);
    } catch (e) {
        console.warn('Failed to get circuit relays:', e);
    }
}
```

**Step 3: Bump UI version**

Update footer version from `UI v0.8.6` to `UI v0.8.7`.

**Step 4: Manual verification**

- Open `webtor-demo/static/index.html` in a browser and click `Open (WebSocket)` to confirm the circuit path still renders.

**Step 5: Commit**

```bash
git add webtor-demo/static/index.html
git commit -m "refactor: simplify circuit display rendering"
```

---

### Task 8: Replace `[` with `[[` in `build.sh`

**Files:**
- Modify: `build.sh`

**Step 1: Update conditionals**

Replace:

```sh
if [ ${#missing_deps[@]} -gt 0 ]; then
```

With:

```sh
if [[ ${#missing_deps[@]} -gt 0 ]]; then
```

Replace:

```sh
if [ "$BUILD_MODE" = "--release" ] && ! command -v wasm-opt &> /dev/null; then
```

With:

```sh
if [[ "$BUILD_MODE" = "--release" ]] && ! command -v wasm-opt &> /dev/null; then
```

Replace each of these:

```sh
if [ $? -ne 0 ]; then
if [ -f "$path" ] && command -v wasm-opt &> /dev/null; then
if [ -f "$path" ]; then
if [ "$BUILD_MODE" = "--release" ]; then
```

With:

```sh
if [[ $? -ne 0 ]]; then
if [[ -f "$path" ]] && command -v wasm-opt &> /dev/null; then
if [[ -f "$path" ]]; then
if [[ "$BUILD_MODE" = "--release" ]]; then
```

**Step 2: Verification**

```bash
bash -n build.sh
```

Expected: no shell syntax errors.

**Step 3: Commit**

```bash
git add build.sh
git commit -m "chore: modernize build.sh conditionals"
```

---

### Task 9: Replace `[` with `[[` in `scripts/fetch-consensus.sh`

**Files:**
- Modify: `scripts/fetch-consensus.sh`

**Step 1: Update conditionals**

Replace:

```sh
if [ "$CONSENSUS_FETCHED" = false ]; then
if [ ${#BATCH[@]} -ge $BATCH_SIZE ]; then
if [ ${#BATCH[@]} -gt 0 ]; then
```

With:

```sh
if [[ "$CONSENSUS_FETCHED" = false ]]; then
if [[ ${#BATCH[@]} -ge $BATCH_SIZE ]]; then
if [[ ${#BATCH[@]} -gt 0 ]]; then
```

**Step 2: Verification**

```bash
bash -n scripts/fetch-consensus.sh
```

Expected: no shell syntax errors.

**Step 3: Commit**

```bash
git add scripts/fetch-consensus.sh
git commit -m "chore: modernize fetch-consensus conditionals"
```

---

### Task 10: Replace `[` with `[[` in `example/build.sh` and `tests/e2e/test_tor.sh`

**Files:**
- Modify: `example/build.sh`
- Modify: `tests/e2e/test_tor.sh`

**Step 1: Update conditionals**

In `example/build.sh` replace:

```sh
if [ ! -f webtor/src/cached/consensus.txt.gz ] || \
   [ $(find webtor/src/cached/consensus.txt.gz -mmin +720 2>/dev/null | wc -l) -gt 0 ]; then
```

With:

```sh
if [[ ! -f webtor/src/cached/consensus.txt.gz ]] || \
   [[ $(find webtor/src/cached/consensus.txt.gz -mmin +720 2>/dev/null | wc -l) -gt 0 ]]; then
```

In `tests/e2e/test_tor.sh` replace:

```sh
if [ $# -lt 2 ]; then
```

With:

```sh
if [[ $# -lt 2 ]]; then
```

**Step 2: Verification**

```bash
bash -n example/build.sh
bash -n tests/e2e/test_tor.sh
```

Expected: no shell syntax errors.

**Step 3: Commit**

```bash
git add example/build.sh tests/e2e/test_tor.sh
git commit -m "chore: modernize example and test_tor scripts"
```

---

### Task 11: Final verification and Sonar follow-up

**Files:**
- No direct changes

**Step 1: Run Rust tests (already run in baseline, re-run after edits)**

```bash
cargo test -p webtor
```

Expected: tests pass (some ignored) and warnings remain unchanged.

**Step 2: Optional E2E run (requires network)**

```bash
npm run test:tls
```

Expected: same results as before, or skipped if network not available.

**Step 3: Confirm SonarCloud**

After pushing, confirm the SonarCloud issues list shows 0 HIGH issues.
