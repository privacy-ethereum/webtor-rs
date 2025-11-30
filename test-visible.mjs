import { chromium } from 'playwright';

async function test() {
  console.log('Starting visible browser test...');
  const browser = await chromium.launch({ headless: false, slowMo: 100 });
  const page = await browser.newPage();
  
  page.on('console', msg => {
    const text = msg.text();
    if (text.includes('TLS') || text.includes('Alert') || text.includes('close_notify') || text.includes('error') || text.includes('Error') || text.includes('Success')) {
      console.log('[BROWSER]', text.substring(0, 200));
    }
  });
  
  try {
    await page.goto('http://localhost:8080', { waitUntil: 'networkidle', timeout: 30000 });
    console.log('Page loaded, clicking Open TorClient...');
    
    await page.locator('button#openBtn').click();
    
    // Wait for circuit
    let ready = false;
    for (let i = 0; i < 90; i++) {
      await page.waitForTimeout(2000);
      const status = await page.locator('#status').textContent();
      if (i % 10 === 0) console.log(`[${i*2}s] ${status}`);
      if (status.includes('Ready')) {
        console.log('Circuit ready!');
        ready = true;
        break;
      }
    }
    
    if (!ready) {
      console.log('Circuit did not become ready');
      await browser.close();
      return;
    }
    
    // Test HTTPS request
    console.log('\n--- Testing HTTPS ---');
    await page.locator('#url1').fill('https://example.com/');
    await page.locator('button#btn1').click();
    
    // Wait and check result
    for (let i = 0; i < 30; i++) {
      await page.waitForTimeout(2000);
      const output = await page.locator('#output1').textContent();
      if (output && output.length > 5) {
        console.log('\n=== RESULT ===');
        console.log(output.substring(0, 500));
        break;
      }
    }
    
    await page.waitForTimeout(5000);
    
  } catch (e) {
    console.error('Error:', e.message);
  } finally {
    await browser.close();
  }
}

test();
