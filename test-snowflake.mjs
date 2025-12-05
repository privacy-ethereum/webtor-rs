import { chromium } from 'playwright';

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  
  // Collect console logs
  page.on('console', msg => {
    const text = msg.text();
    if (text.includes('INFO') || text.includes('WARN') || text.includes('ERR')) {
      console.log(text);
    }
  });

  console.log('Opening demo page...');
  await page.goto('http://localhost:5173/', { waitUntil: 'networkidle' });
  
  // Wait for the app to load
  await page.waitForTimeout(2000);
  
  // Click Enable Webtor button to start connection
  console.log('Clicking Enable Webtor...');
  const enableBtn = await page.$('button:has-text("Enable Webtor")');
  if (enableBtn) {
    await enableBtn.click();
  } else {
    console.log('Enable Webtor button not found');
  }
  
  // Wait for connection attempt
  console.log('Waiting for connection logs...');
  await page.waitForTimeout(45000); // Wait up to 45s
  
  await browser.close();
  console.log('Test complete');
})();
