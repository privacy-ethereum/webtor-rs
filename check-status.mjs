import { chromium } from 'playwright';

async function checkStatus() {
  const browser = await chromium.launch({ headless: false });
  const context = await browser.newContext();
  const page = await context.newPage();
  
  let instantErrorFound = false;
  
  page.on('console', msg => {
    const text = msg.text();
    if (text.includes('std::time::Instant') || text.includes('Instant not available')) {
      instantErrorFound = true;
      console.log('CRITICAL: INSTANT ERROR: ' + text);
    }
  });
  
  try {
    await page.goto('http://localhost:8080', { waitUntil: 'networkidle', timeout: 30000 });
    await page.waitForTimeout(2000);
    
    await page.locator('button#openBtn').click();
    console.log('Opened TorClient, waiting for circuit...');
    
    await page.waitForTimeout(90000);
    
    const statusText = await page.locator('#status').textContent();
    console.log('\nCircuit Status: ' + statusText);
    
    await page.screenshot({ path: '/Users/user/pse/webtor-rs/screenshot-status.png', fullPage: true });
    
    console.log('\nMaking HTTP request...');
    await page.locator('button:has-text("Make Request 1")').first().click();
    
    await page.waitForTimeout(90000);
    
    const requestResult = await page.locator('text=Request failed').count();
    const successResult = await page.locator('text=Response received').count();
    
    console.log('\n=== RESULTS ===');
    console.log('Instant Error Found: ' + (instantErrorFound ? 'YES (FAIL)' : 'NO (PASS)'));
    console.log('Request Failed Messages: ' + requestResult);
    console.log('Request Success Messages: ' + successResult);
    
    if (!instantErrorFound) {
      console.log('\nSUCCESS: std::time::Instant error has been RESOLVED!');
    } else {
      console.log('\nFAILURE: std::time::Instant error still exists');
    }
    
  } catch (error) {
    console.error('Error:', error.message);
  } finally {
    await page.waitForTimeout(5000);
    await browser.close();
  }
}

checkStatus().catch(console.error);
