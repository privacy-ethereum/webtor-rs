import { chromium } from 'playwright';

async function test() {
  console.log('Starting headless test...');
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  
  const logs = [];
  page.on('console', msg => {
    const text = msg.text();
    logs.push(text);
    if (text.includes('TLS') || text.includes('handshake') || text.includes('ALPN') || text.includes('Alert') || text.includes('close_notify')) {
      console.log('[LOG]', text.substring(0, 300));
    }
  });
  
  try {
    await page.goto('http://localhost:8080', { waitUntil: 'networkidle', timeout: 30000 });
    console.log('Page loaded');
    
    // Open TorClient
    await page.locator('button#openBtn').click();
    console.log('Clicked Open TorClient');
    
    // Wait for circuit
    for (let i = 0; i < 60; i++) {
      await page.waitForTimeout(2000);
      const status = await page.locator('#status').textContent();
      if (i % 5 === 0) console.log(`[${i*2}s] Status: ${status}`);
      if (status.includes('Ready')) {
        console.log('Circuit ready!');
        break;
      }
    }
    
    // Make HTTPS request
    const urlInput = await page.locator('#url1');
    await urlInput.fill('https://example.com/');
    console.log('\nMaking HTTPS request to example.com...');
    await page.locator('button#btn1').click();
    
    // Wait for result
    for (let i = 0; i < 30; i++) {
      await page.waitForTimeout(2000);
      const output = await page.locator('#output1').textContent();
      if (output && output.length > 10) {
        console.log('\n=== RESULT ===');
        console.log(output.substring(0, 800));
        break;
      }
    }
    
    // Print TLS logs
    console.log('\n=== TLS Logs ===');
    logs.filter(l => l.includes('TLS') || l.includes('ClientHello') || l.includes('ALPN') || l.includes('extension')).slice(-15).forEach(l => console.log(l.substring(0, 250)));
    
  } catch (e) {
    console.error('Error:', e.message);
  } finally {
    await browser.close();
  }
}

test();
