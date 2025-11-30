import { chromium } from 'playwright';

async function testHttpsWithAlpn() {
  console.log('=== Testing HTTPS Request with ALPN Extension ===\n');
  
  const browser = await chromium.launch({ headless: false });
  const context = await browser.newContext();
  const page = await context.newPage();
  
  // Collect logs
  const tlsLogs = [];
  page.on('console', msg => {
    const text = msg.text();
    console.log('[console]', text.substring(0, 200));
    if (text.includes('TLS') || text.includes('handshake') || text.includes('Alert') || text.includes('close_notify')) {
      tlsLogs.push(text);
    }
  });
  
  try {
    await page.goto('http://localhost:8080', { waitUntil: 'networkidle', timeout: 30000 });
    await page.waitForTimeout(2000);
    
    // Enable debug logging
    const debugToggle = await page.locator('#debugToggle');
    await debugToggle.check();
    console.log('Debug logging enabled');
    
    // Open TorClient
    console.log('Opening TorClient...');
    await page.locator('button#openBtn').click();
    
    // Wait for circuit
    console.log('Waiting for circuit (max 120s)...');
    
    let ready = false;
    for (let i = 0; i < 60; i++) {
      await page.waitForTimeout(2000);
      const statusText = await page.locator('#status').textContent();
      if (statusText.includes('Ready')) {
        console.log('Circuit is ready!');
        ready = true;
        break;
      }
    }
    
    if (!ready) {
      console.log('Circuit not ready after 120s');
      await browser.close();
      return;
    }
    
    // Test HTTPS request to example.com (simpler than httpbin.org)
    console.log('\n=== Testing HTTPS request to example.com ===');
    const urlInput = await page.locator('#url1');
    await urlInput.fill('https://example.com/');
    
    console.log('Making HTTPS request...');
    await page.locator('button#btn1').click();
    
    // Wait for response
    await page.waitForTimeout(90000);
    
    const output = await page.locator('#output1').textContent();
    console.log('\n=== Result ===');
    console.log('Output:', output);
    
    if (output.includes('Success')) {
      console.log('\n✅ HTTPS request SUCCEEDED! ALPN fix worked!');
    } else if (output.includes('TLS') && output.includes('failed')) {
      console.log('\n❌ HTTPS TLS handshake still failing');
      console.log('TLS logs:', tlsLogs.slice(-10).join('\n'));
    } else {
      console.log('\n⏳ Result unclear');
    }
    
    // Screenshot
    await page.screenshot({ path: '/Users/user/pse/webtor-rs/screenshot-alpn-test.png', fullPage: true });
    
  } catch (error) {
    console.error('Test error:', error.message);
  } finally {
    await page.waitForTimeout(3000);
    await browser.close();
  }
}

testHttpsWithAlpn().catch(console.error);
