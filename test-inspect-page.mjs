import { chromium } from 'playwright';

async function inspectPage() {
  const browser = await chromium.launch({ headless: false });
  const context = await browser.newContext();
  const page = await context.newPage();
  
  try {
    console.log('Navigating to http://localhost:8080');
    await page.goto('http://localhost:8080', { waitUntil: 'networkidle', timeout: 30000 });
    
    console.log('Waiting for page to load');
    await page.waitForTimeout(3000);
    
    console.log('\n=== Page Title ===');
    console.log(await page.title());
    
    console.log('\n=== All Buttons ===');
    const buttons = await page.locator('button').all();
    console.log('Found ' + buttons.length + ' buttons:');
    for (let i = 0; i < buttons.length; i++) {
      const text = await buttons[i].textContent();
      const visible = await buttons[i].isVisible();
      console.log('  Button ' + i + ': "' + text + '" (visible: ' + visible + ')');
    }
    
    console.log('\n=== All Input Fields ===');
    const inputs = await page.locator('input').all();
    console.log('Found ' + inputs.length + ' inputs:');
    for (let i = 0; i < inputs.length; i++) {
      const type = await inputs[i].getAttribute('type');
      const placeholder = await inputs[i].getAttribute('placeholder');
      const visible = await inputs[i].isVisible();
      console.log('  Input ' + i + ': type="' + type + '" placeholder="' + placeholder + '" (visible: ' + visible + ')');
    }
    
    console.log('\n=== Page Body HTML (first 2000 chars) ===');
    const bodyHTML = await page.locator('body').innerHTML();
    console.log(bodyHTML.substring(0, 2000));
    
    await page.screenshot({ path: '/Users/user/pse/webtor-rs/screenshot-inspect.png', fullPage: true });
    console.log('\nFull page screenshot saved: screenshot-inspect.png');
    
    console.log('\nKeeping browser open for 20 seconds for manual inspection...');
    await page.waitForTimeout(20000);
    
  } catch (error) {
    console.error('Error:', error.message);
  } finally {
    await browser.close();
  }
}

inspectPage().catch(console.error);
