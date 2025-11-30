import { chromium } from 'playwright';

async function testHttpOnly() {
  console.log('=== Testing HTTP (non-HTTPS) Request Through Tor ===\n');
  
  const browser = await chromium.launch({ headless: false });
  const context = await browser.newContext();
  const page = await context.newPage();
  
  // Collect console logs
  const logs = [];
  page.on('console', msg => {
    const text = msg.text();
    logs.push(text);
    console.log('[console]', text);
  });
  
  try {
    await page.goto('http://localhost:8080', { waitUntil: 'networkidle', timeout: 30000 });
    await page.waitForTimeout(2000);
    
    // Click Open TorClient
    console.log('Opening TorClient...');
    await page.locator('button#openBtn').click();
    
    // Wait for circuit to be ready
    console.log('Waiting for circuit to be ready (max 120s)...');
    await page.waitForTimeout(120000);
    
    const statusText = await page.locator('#status').textContent();
    console.log('\nCircuit Status:', statusText);
    
    if (!statusText.includes('Ready')) {
      console.log('Circuit not ready, aborting test');
      await browser.close();
      return;
    }
    
    // Change URL to plain HTTP endpoint (example.com, not HTTPS)
    console.log('\n=== Testing HTTP (non-HTTPS) request ===');
    const url1Input = await page.locator('#url1');
    await url1Input.fill('http://example.com/');
    
    // Make the request
    console.log('Making HTTP request to http://example.com/...');
    await page.locator('button#btn1').click();
    
    // Wait for response
    await page.waitForTimeout(60000);
    
    // Check result
    const output1 = await page.locator('#output1').textContent();
    console.log('\n=== HTTP Request Result ===');
    console.log('Output:', output1);
    
    if (output1.includes('Success')) {
      console.log('\n✅ HTTP (non-HTTPS) request SUCCEEDED!');
      console.log('This confirms the Tor circuit is working.');
    } else if (output1.includes('failed') || output1.includes('error')) {
      console.log('\n❌ HTTP request FAILED');
    } else {
      console.log('\n⏳ Request may still be in progress');
    }
    
    // Now try HTTPS to see if that fails specifically
    console.log('\n=== Testing HTTPS request ===');
    await url1Input.fill('https://example.com/');
    await page.locator('button#btn1').click();
    await page.waitForTimeout(60000);
    
    const output2 = await page.locator('#output1').textContent();
    console.log('\n=== HTTPS Request Result ===');
    console.log('Output:', output2);
    
    if (output2.includes('Success')) {
      console.log('\n✅ HTTPS request SUCCEEDED!');
    } else if (output2.includes('failed') || output2.includes('error')) {
      console.log('\n❌ HTTPS request FAILED (TLS issue)');
    }
    
    // Screenshot
    await page.screenshot({ path: '/Users/user/pse/webtor-rs/screenshot-http-test.png', fullPage: true });
    
  } catch (error) {
    console.error('Test error:', error.message);
  } finally {
    await page.waitForTimeout(3000);
    await browser.close();
  }
}

testHttpOnly().catch(console.error);
