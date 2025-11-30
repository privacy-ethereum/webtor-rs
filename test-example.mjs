#!/usr/bin/env node
/**
 * Headless browser test for webtor-rs example app
 */

import { chromium } from 'playwright';

const CONFIG = {
    url: 'http://localhost:5173',
    timeout: 300000, // 5 minutes for Snowflake Tor connection
};

async function runTest() {
    let browser = null;
    
    try {
        console.log('🌐 Launching headless browser...');
        browser = await chromium.launch({ headless: true });
        
        const context = await browser.newContext();
        const page = await context.newPage();
        
        // Collect console logs
        page.on('console', msg => {
            const text = msg.text();
            const type = msg.type();
            if (type === 'error') {
                console.log(`❌ [error] ${text}`);
            } else if (text.includes('INFO') || text.includes('WARN') || text.includes('ERROR') ||
                       text.includes('circuit') || text.includes('Channel') || text.includes('consensus') ||
                       text.includes('WebTunnel') || text.includes('✅') || text.includes('❌')) {
                console.log(`   [log] ${text.substring(0, 200)}`);
            }
        });
        
        page.on('pageerror', error => {
            console.log(`❌ [page error] ${error.message}`);
        });
        
        console.log(`📄 Loading ${CONFIG.url}...`);
        await page.goto(CONFIG.url, { timeout: 30000 });
        
        // Wait for page to load
        await page.waitForSelector('button:has-text("Enable Webtor")', { timeout: 10000 });
        console.log('✅ Page loaded');
        
        // Click Enable Webtor button
        console.log('🔓 Clicking "Enable Webtor"...');
        await page.click('button:has-text("Enable Webtor")');
        
        // Wait for connection (up to 3 minutes)
        console.log('⏳ Waiting for Tor connection (may take 30-120 seconds)...');
        
        const startTime = Date.now();
        let connected = false;
        let lastStatus = '';
        
        while (Date.now() - startTime < CONFIG.timeout) {
            // Check badge text for status
            try {
                const badge = await page.$('span.chakra-badge');
                if (badge) {
                    const status = await badge.textContent();
                    if (status !== lastStatus) {
                        lastStatus = status;
                        console.log(`   Status: ${status}`);
                    }
                    
                    if (status.includes('Connected')) {
                        connected = true;
                        break;
                    }
                    
                    if (status.includes('failed') || status.includes('Failed')) {
                        throw new Error(`Connection failed: ${status}`);
                    }
                }
            } catch (e) {
                // Ignore selector errors
            }
            
            await new Promise(r => setTimeout(r, 2000));
        }
        
        if (!connected) {
            // Get logs from the log panel
            const logs = await page.$$eval('[class*="chakra-box"] span', els => 
                els.map(el => el.textContent).filter(t => t && t.length > 0).join('\n')
            );
            console.log('\n📋 Logs from page:\n', logs);
            throw new Error('Timeout waiting for connection');
        }
        
        const connectionTime = ((Date.now() - startTime) / 1000).toFixed(1);
        console.log(`✅ Connected in ${connectionTime}s`);
        
        // Click Fetch button
        console.log('🌐 Clicking "Fetch My Tor IP"...');
        await page.click('button:has-text("Fetch My Tor IP")');
        
        // Wait for result
        await page.waitForSelector('[class*="chakra-alert"]', { timeout: 60000 });
        
        const alertText = await page.$eval('[class*="chakra-alert"]', el => el.textContent);
        console.log(`   Result: ${alertText}`);
        
        if (alertText.includes('Exit IP')) {
            console.log('\n🎉 TEST PASSED');
            return { success: true };
        } else {
            throw new Error(`Unexpected result: ${alertText}`);
        }
        
    } catch (error) {
        console.log(`\n❌ TEST FAILED: ${error.message}`);
        return { success: false, error: error.message };
        
    } finally {
        if (browser) {
            await browser.close();
        }
    }
}

runTest().then(result => {
    process.exit(result.success ? 0 : 1);
}).catch(err => {
    console.error('Fatal error:', err);
    process.exit(1);
});
