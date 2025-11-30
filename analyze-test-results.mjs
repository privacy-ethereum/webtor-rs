import { chromium } from 'playwright';

async function analyzeErrors() {
  const browser = await chromium.launch({ headless: false });
  const context = await browser.newContext();
  const page = await context.newPage();
  
  const consoleMessages = [];
  const instantErrors = [];
  const wasmPanics = [];
  
  page.on('console', msg => {
    const msgType = msg.type();
    const text = msg.text();
    consoleMessages.push({ type: msgType, text });
    
    if (text.includes('std::time::Instant') || text.includes('Instant not available')) {
      instantErrors.push(text);
      console.log('INSTANT ERROR DETECTED: ' + text);
    }
    
    if (text.includes('panicked at') && !text.includes('no panic')) {
      wasmPanics.push(text);
      console.log('WASM PANIC DETECTED: ' + text);
    }
  });
  
  page.on('pageerror', error => {
    console.error('PAGE ERROR: ' + error.message);
  });
  
  try {
    console.log('Navigating to http://localhost:8080');
    await page.goto('http://localhost:8080', { waitUntil: 'networkidle', timeout: 30000 });
    await page.waitForTimeout(2000);
    
    console.log('Clicking Open TorClient button');
    await page.locator('button#openBtn').click();
    
    console.log('Waiting 90 seconds for circuit establishment');
    await page.waitForTimeout(90000);
    
    console.log('Clicking Make Request 1 button');
    const req1Btn = page.locator('button:has-text("Make Request 1")').first();
    await req1Btn.click();
    
    console.log('Waiting 90 seconds for request completion');
    await page.waitForTimeout(90000);
    
    await page.screenshot({ path: '/Users/user/pse/webtor-rs/screenshot-final-test.png' });
    
    console.log('\n=== FINAL ANALYSIS ===');
    console.log('Total console messages: ' + consoleMessages.length);
    console.log('Instant errors: ' + instantErrors.length);
    console.log('WASM panics: ' + wasmPanics.length);
    
    if (instantErrors.length === 0) {
      console.log('\nPASS: No std::time::Instant errors detected!');
    } else {
      console.log('\nFAIL: Found ' + instantErrors.length + ' Instant errors:');
      instantErrors.forEach(err => console.log('  - ' + err));
    }
    
    if (wasmPanics.length === 0) {
      console.log('PASS: No WASM panics detected!');
    } else {
      console.log('FAIL: Found ' + wasmPanics.length + ' WASM panics:');
      wasmPanics.forEach(err => console.log('  - ' + err));
    }
    
    const errorMsgs = consoleMessages.filter(m => m.type === 'error' || m.text.includes('error'));
    console.log('\nAll error messages (' + errorMsgs.length + '):');
    errorMsgs.slice(-10).forEach(m => console.log('  [' + m.type + '] ' + m.text));
    
  } catch (error) {
    console.error('Test error:', error.message);
  } finally {
    console.log('\nKeeping browser open for 10 seconds');
    await page.waitForTimeout(10000);
    await browser.close();
  }
}

analyzeErrors().catch(console.error);
