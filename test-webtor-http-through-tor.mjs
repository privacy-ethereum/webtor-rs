import { chromium } from 'playwright';
import { writeFileSync } from 'fs';

async function testWebtorHttpThroughTor() {
    console.log('='.repeat(80));
    console.log('WEBTOR-RS HTTP THROUGH TOR - FUNCTIONAL BROWSER TEST');
    console.log('Testing fix for std::time::Instant WASM panic issue');
    console.log('='.repeat(80));
    console.log('');
    
    const browser = await chromium.launch({ 
        headless: false,
        args: ['--disable-web-security'],
        slowMo: 100
    });
    
    const context = await browser.newContext({
        viewport: { width: 1920, height: 1080 }
    });
    
    const page = await context.newPage();
    
    const consoleMessages = [];
    page.on('console', msg => {
        const msgType = msg.type();
        const text = msg.text();
        consoleMessages.push({ type: msgType, text: text, timestamp: new Date().toISOString() });
        
        if (msgType === 'error' || text.includes('panic') || text.includes('Instant')) {
            console.log('[Browser ' + msgType + '] ' + text);
        }
    });
    
    const pageErrors = [];
    page.on('pageerror', error => {
        pageErrors.push({ message: error.message, stack: error.stack, timestamp: new Date().toISOString() });
        console.error('\n' + '!'.repeat(80));
        console.error('PAGE ERROR DETECTED (POTENTIAL WASM PANIC):');
        console.error(error.message);
        console.error('!'.repeat(80) + '\n');
    });
    
    const testResults = {
        wasmLoaded: false,
        torClientOpened: false,
        circuitEstablished: false,
        httpRequestSuccess: false,
        httpRequestError: null,
        responseReceived: false,
        instantPanicDetected: false,
        screenshots: []
    };
    
    try {
        console.log('[1/8] Navigating to http://localhost:8080...');
        await page.goto('http://localhost:8080', { 
            waitUntil: 'networkidle',
            timeout: 30000 
        });
        console.log('      Page loaded successfully\n');
        
        await page.screenshot({ path: '/tmp/webtor-test-01-initial.png', fullPage: true });
        testResults.screenshots.push('/tmp/webtor-test-01-initial.png');
        console.log('      Screenshot: /tmp/webtor-test-01-initial.png\n');
        
        console.log('[2/8] Waiting for WASM module initialization...');
        await page.waitForTimeout(3000);
        
        const wasmLoaded = await page.evaluate(() => {
            return typeof window.demoApp !== 'undefined';
        });
        testResults.wasmLoaded = wasmLoaded;
        console.log('      WASM Loaded: ' + (wasmLoaded ? 'YES' : 'NO') + '\n');
        
        if (!wasmLoaded) {
            throw new Error('WASM module failed to load');
        }
        
        await page.screenshot({ path: '/tmp/webtor-test-02-wasm-loaded.png', fullPage: true });
        testResults.screenshots.push('/tmp/webtor-test-02-wasm-loaded.png');
        
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
            console.log('      [' + (waitTime/1000) + 's] Status: ' + statusText.trim());
            
            if (statusText.includes('Circuit ready') || statusText.includes('Connected')) {
                circuitReady = true;
                testResults.circuitEstablished = true;
                console.log('      Circuit established!\n');
            }
            
            if (pageErrors.length > 0) {
                const instantError = pageErrors.find(e => 
                    e.message.includes('Instant') || 
                    e.message.includes('std::time')
                );
                if (instantError) {
                    testResults.instantPanicDetected = true;
                    throw new Error('WASM panic detected: std::time::Instant issue NOT FIXED');
                }
            }
        }
        
        if (!circuitReady) {
            console.log('      Warning: Circuit did not establish within timeout, proceeding anyway...\n');
        }
        
        await page.screenshot({ path: '/tmp/webtor-test-03-circuit-status.png', fullPage: true });
        testResults.screenshots.push('/tmp/webtor-test-03-circuit-status.png');
        
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
                console.log('      [' + (requestWaitTime/1000) + 's] Waiting for response...');
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
            
            if (pageErrors.length > 0) {
                const instantError = pageErrors.find(e => 
                    e.message.includes('Instant') || 
                    e.message.includes('std::time')
                );
                if (instantError) {
                    testResults.instantPanicDetected = true;
                    testResults.httpRequestError = instantError.message;
                    throw new Error('WASM panic during HTTP request: std::time::Instant issue NOT FIXED');
                }
            }
        }
        
        await page.screenshot({ path: '/tmp/webtor-test-05-after-request.png', fullPage: true });
        testResults.screenshots.push('/tmp/webtor-test-05-after-request.png');
        
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
            consoleMessageCount: consoleMessages.length
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
        
        await browser.close();
        console.log('Browser closed.');
        console.log('Test complete.\n');
    }
}

testWebtorHttpThroughTor().catch(error => {
    console.error('Fatal test error:', error);
    process.exit(1);
});
