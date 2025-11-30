import { chromium } from 'playwright';
import { writeFileSync } from 'fs';

async function testWebtorDemo() {
    console.log('Starting Webtor-rs WASM Demo Test...\n');
    
    const browser = await chromium.launch({ 
        headless: true,
        args: ['--disable-web-security']
    });
    
    const context = await browser.newContext({
        viewport: { width: 1280, height: 720 }
    });
    
    const page = await context.newPage();
    
    const consoleMessages = [];
    page.on('console', msg => {
        const msgType = msg.type();
        const text = msg.text();
        consoleMessages.push({ type: msgType, text: text });
        console.log('[Browser ' + msgType + '] ' + text);
    });
    
    const pageErrors = [];
    page.on('pageerror', error => {
        pageErrors.push(error.message);
        console.error('[Page Error] ' + error.message);
    });
    
    try {
        console.log('1. Navigating to http://localhost:8000...');
        await page.goto('http://localhost:8000', { 
            waitUntil: 'networkidle',
            timeout: 30000 
        });
        
        console.log('2. Taking initial screenshot...');
        await page.screenshot({ path: '/tmp/webtor-demo-initial.png', fullPage: true });
        console.log('   Screenshot saved to /tmp/webtor-demo-initial.png');
        
        console.log('3. Waiting for WASM module to load...');
        await page.waitForTimeout(3000);
        
        console.log('4. Checking page title...');
        const title = await page.title();
        console.log('   Page title: "' + title + '"');
        
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
        
        console.log('6. Checking for WASM initialization...');
        const wasmLoaded = await page.evaluate(() => {
            return typeof window.demoApp !== 'undefined';
        });
        console.log('   - WASM App Object: ' + (wasmLoaded ? 'Initialized' : 'NOT FOUND'));
        
        console.log('7. Taking post-load screenshot...');
        await page.screenshot({ path: '/tmp/webtor-demo-loaded.png', fullPage: true });
        console.log('   Screenshot saved to /tmp/webtor-demo-loaded.png');
        
        if (wasmLoaded && openBtn) {
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
        } else {
            console.log('8. SKIPPING button click - WASM not loaded or button not found');
        }
        
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

testWebtorDemo().catch(console.error);
