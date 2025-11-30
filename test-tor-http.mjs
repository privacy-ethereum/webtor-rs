import { chromium } from 'playwright';

async function testTorHttpFlow() {
  const browser = await chromium.launch({ headless: false });
  const context = await browser.newContext();
  const page = await context.newPage();
  
  const consoleMessages = [];
  page.on('console', msg => {
    const msgType = msg.type();
    const text = msg.text();
    consoleMessages.push({ type: msgType, text });
    console.log('[' + msgType + '] ' + text);
  });
  
  const pageErrors = [];
  page.on('pageerror', error => {
    pageErrors.push(error.message);
    console.error('PAGE ERROR:', error.message);
  });
  
  try {
    console.log('\n=== STEP 1: Navigate to page ===');
    await page.goto('http://localhost:8080', { waitUntil: 'networkidle', timeout: 30000 });
    await page.waitForTimeout(2000);
    
    console.log('\n=== STEP 2: Click Open TorClient button ===');
    const openBtn = page.locator('button#openBtn');
    await openBtn.click();
    console.log('Open TorClient button clicked');
    
    console.log('\n=== STEP 3: Wait for circuit establishment (up to 120 seconds) ===');
    await page.waitForTimeout(5000);
    
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
    
    if (!circuitReady) {
      console.log('WARNING: Circuit may not be ready yet, continuing anyway...');
    }
    
    await page.screenshot({ path: '/Users/user/pse/webtor-rs/screenshot-circuit-ready.png' });
    console.log('Screenshot saved: screenshot-circuit-ready.png');
    
    console.log('\n=== STEP 4: Make HTTP request through Tor ===');
    const requestBtn = page.locator('button:has-text("Make Request 1")').first();
    await requestBtn.waitFor({ state: 'visible', timeout: 5000 });
    
    console.log('Clicking Make Request 1 button');
    await requestBtn.click();
    
    console.log('\n=== STEP 5: Wait for HTTP response (60 seconds) ===');
    await page.waitForTimeout(60000);
    
    await page.screenshot({ path: '/Users/user/pse/webtor-rs/screenshot-after-request.png' });
    console.log('Screenshot saved: screenshot-after-request.png');
    
    console.log('\n=== TEST ANALYSIS ===');
    console.log('Total console messages: ' + consoleMessages.length);
    console.log('Total page errors: ' + pageErrors.length);
    
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
    
    console.log('\n--- CRITICAL ISSUES ---');
    if (instantErrors.length > 0) {
      console.log('FAIL: std::time::Instant errors found (' + instantErrors.length + '):');
      instantErrors.forEach(err => console.log('  - ' + err.text));
    } else {
      console.log('PASS: No std::time::Instant errors');
    }
    
    if (wasmPanics.length > 0) {
      console.log('FAIL: WASM panics found (' + wasmPanics.length + '):');
      wasmPanics.forEach(err => console.log('  - ' + err.text));
    } else {
      console.log('PASS: No WASM panics');
    }
    
    console.log('\n--- HTTP REQUEST STATUS ---');
    if (httpSuccess.length > 0) {
      console.log('PASS: HTTP request succeeded (' + httpSuccess.length + ' success messages)');
      httpSuccess.slice(0, 3).forEach(msg => console.log('  - ' + msg.text));
    } else if (httpErrors.length > 0) {
      console.log('FAIL: HTTP request failed (' + httpErrors.length + ' error messages)');
      httpErrors.slice(0, 3).forEach(msg => console.log('  - ' + msg.text));
    } else {
      console.log('UNKNOWN: No clear success or failure messages found');
    }
    
    if (pageErrors.length > 0) {
      console.log('\n--- PAGE ERRORS ---');
      pageErrors.forEach(err => console.log('  - ' + err));
    }
    
    console.log('\n--- LAST 30 CONSOLE MESSAGES ---');
    consoleMessages.slice(-30).forEach(msg => {
      console.log('[' + msg.type + '] ' + msg.text);
    });
    
    console.log('\n=== FINAL VERDICT ===');
    const hasInstantError = instantErrors.length > 0;
    const hasPanic = wasmPanics.length > 0;
    const requestSucceeded = httpSuccess.length > 0;
    
    if (!hasInstantError && !hasPanic && requestSucceeded) {
      console.log('SUCCESS: All tests passed - HTTP request works without WASM errors');
    } else if (hasInstantError) {
      console.log('FAILED: std::time::Instant error still present');
    } else if (hasPanic) {
      console.log('FAILED: WASM panic occurred');
    } else if (!requestSucceeded) {
      console.log('INCONCLUSIVE: No Instant errors but HTTP request status unclear');
    }
    
  } catch (error) {
    console.error('TEST FAILED:', error.message);
    await page.screenshot({ path: '/Users/user/pse/webtor-rs/screenshot-test-error.png' });
  } finally {
    console.log('\nKeeping browser open for 15 seconds...');
    await page.waitForTimeout(15000);
    await browser.close();
  }
}

testTorHttpFlow().catch(console.error);
