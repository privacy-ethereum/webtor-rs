import { chromium } from 'playwright';

async function test() {
  console.log('Testing httpbin.org...');
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  
  page.on('console', msg => {
    const text = msg.text();
    if (text.includes('TLS') || text.includes('Alert') || text.includes('close_notify') || text.includes('error') || text.includes('Error') || text.includes('httpbin')) {
      console.log('[LOG]', text.substring(0, 300));
    }
  });
  
  try {
    await page.goto('http://localhost:9000', { waitUntil: 'networkidle', timeout: 30000 });
    await page.locator('button#openBtn').click();
    console.log('Opening TorClient...');
    
    // Wait for circuit
    for (let i = 0; i < 60; i++) {
      await page.waitForTimeout(2000);
      const status = await page.locator('#status').textContent();
      if (status.includes('Ready')) {
        console.log('Circuit ready!');
        break;
      }
    }
    
    // Test httpbin.org
    console.log('\n--- Testing https://httpbin.org/user-agent ---');
    await page.locator('#url1').fill('https://httpbin.org/user-agent');
    await page.locator('button#btn1').click();
    
    // Wait for result
    for (let i = 0; i < 45; i++) {
      await page.waitForTimeout(2000);
      const output = await page.locator('#output1').textContent();
      if (output && output.length > 5) {
        console.log('\n=== RESULT ===');
        console.log(output);
        break;
      }
    }
    
  } catch (e) {
    console.error('Error:', e.message);
  } finally {
    await browser.close();
  }
}

test();
