import { chromium } from 'playwright';

async function testHttpFlow() {
  const browser = await chromium.launch({ headless: false });
  const context = await browser.newContext();
  const page = await context.newPage();
  
  // Listen for console messages
  const consoleMessages = [];
  page.on('console', msg => {
    const msgType = msg.type();
    const text = msg.text();
    consoleMessages.push({ type: msgType, text });
    console.log('[' + msgType + '] ' + text);
  });
  
  // Listen for page errors
  const pageErrors = [];
  page.on('pageerror', error => {
    pageErrors.push(error.message);
    console.error('Page error:', error.message);
  });
  
  try {
    console.log('Step 1: Navigating to http://localhost:8080');
    await page.goto('http://localhost:8080', { waitUntil: 'networkidle', timeout: 30000 });
    
    console.log('Step 2: Waiting for page to load completely');
    await page.waitForTimeout(2000);
    
    console.log('Step 3: Looking for Connect button');
    const connectButton = await page.locator('button:has-text("Connect")').first();
    await connectButton.waitFor({ state: 'visible', timeout: 10000 });
    
    console.log('Step 4: Clicking Connect button');
    await connectButton.click();
    
    console.log('Step 5: Waiting for circuit establishment (up to 120 seconds)');
    const statusIndicator = page.locator('text=/Ready|Connected|Circuit established/i').first();
    
    try {
      await statusIndicator.waitFor({ state: 'visible', timeout: 120000 });
      console.log('Circuit established - Ready status found');
    } catch (e) {
      console.log('Waiting longer for circuit establishment...');
      await page.waitForTimeout(10000);
    }
    
    await page.screenshot({ path: '/Users/user/pse/webtor-rs/screenshot-connected.png' });
    console.log('Screenshot saved: screenshot-connected.png');
    
    console.log('Step 6: Looking for HTTP request input field');
    let requestInput = null;
    const possibleSelectors = [
      'input[type="text"]',
      'input[placeholder*="URL"]',
      'input[placeholder*="url"]',
      'input[placeholder*="request"]',
      '#url-input',
      '#request-input'
    ];
    
    for (const selector of possibleSelectors) {
      const element = page.locator(selector).first();
      if (await element.count() > 0) {
        requestInput = element;
        console.log('Found input using selector: ' + selector);
        break;
      }
    }
    
    if (!requestInput) {
      console.log('No input field found, checking page content');
      const pageContent = await page.content();
      console.log('Page HTML preview:', pageContent.substring(0, 1000));
    }
    
    console.log('Step 7: Looking for submit/request button');
    let submitButton = null;
    const buttonSelectors = [
      'button:has-text("Request")',
      'button:has-text("Send")',
      'button:has-text("Submit")',
      'button:has-text("Fetch")',
      'button[type="submit"]'
    ];
    
    for (const selector of buttonSelectors) {
      const element = page.locator(selector).first();
      if (await element.count() > 0) {
        submitButton = element;
        console.log('Found button using selector: ' + selector);
        break;
      }
    }
    
    if (requestInput && submitButton) {
      console.log('Step 8: Entering test URL');
      await requestInput.fill('http://example.com');
      
      console.log('Step 9: Clicking submit button to make HTTP request');
      await submitButton.click();
      
      console.log('Step 10: Waiting for response (up to 60 seconds)');
      await page.waitForTimeout(60000);
      
      await page.screenshot({ path: '/Users/user/pse/webtor-rs/screenshot-after-request.png' });
      console.log('Screenshot saved: screenshot-after-request.png');
    } else {
      console.log('Could not find request input/button, waiting to observe page state');
      await page.waitForTimeout(10000);
      await page.screenshot({ path: '/Users/user/pse/webtor-rs/screenshot-no-input.png' });
    }
    
    console.log('\n=== TEST RESULTS ===');
    console.log('Total console messages: ' + consoleMessages.length);
    console.log('Total page errors: ' + pageErrors.length);
    
    const instantErrors = consoleMessages.filter(m => 
      m.text.includes('std::time::Instant') || 
      m.text.includes('Instant not available')
    );
    
    const wasmPanics = consoleMessages.filter(m =>
      m.text.includes('panicked at') || 
      m.text.includes('panic')
    );
    
    console.log('\n--- Critical Issues ---');
    if (instantErrors.length > 0) {
      console.log('INSTANT ERRORS FOUND:');
      instantErrors.forEach(err => console.log('  ' + err.text));
    } else {
      console.log('No std::time::Instant errors detected');
    }
    
    if (wasmPanics.length > 0) {
      console.log('WASM PANICS FOUND:');
      wasmPanics.forEach(err => console.log('  ' + err.text));
    } else {
      console.log('No WASM panics detected');
    }
    
    if (pageErrors.length > 0) {
      console.log('PAGE ERRORS:');
      pageErrors.forEach(err => console.log('  ' + err));
    } else {
      console.log('No page errors detected');
    }
    
    console.log('\n--- Recent Console Messages (last 20) ---');
    consoleMessages.slice(-20).forEach(msg => {
      console.log('[' + msg.type + '] ' + msg.text);
    });
    
  } catch (error) {
    console.error('Test failed with error:', error.message);
    await page.screenshot({ path: '/Users/user/pse/webtor-rs/screenshot-error.png' });
  } finally {
    console.log('\nKeeping browser open for 10 seconds for inspection...');
    await page.waitForTimeout(10000);
    await browser.close();
  }
}

testHttpFlow().catch(console.error);
