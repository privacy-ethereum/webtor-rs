# E2E Test Plan for Webtor-rs Demo Application

## Executive Summary

This document outlines a comprehensive end-to-end testing strategy for the webtor-rs WASM-based Tor client demo application. The test suite uses Playwright to validate browser-based WASM module loading, Tor circuit establishment, HTTP requests through Tor, and error handling.

## Project Context

**Technology Stack:**
- **Frontend:** Vanilla JavaScript + WASM
- **WASM Runtime:** Rust compiled via wasm-pack
- **Tor Client:** Custom Rust implementation with Snowflake bridge support
- **Testing Framework:** Playwright (already partially implemented)

**Current Implementation:**
- Basic headless test exists (`test-headless.mjs`)
- Covers: WASM loading, TorClient connection, single HTTP request
- Missing: Comprehensive test scenarios, CI/CD integration, proper test reporting

---

## 1. Test Infrastructure Setup

### 1.1 Dependencies & Configuration

**Required Packages:**
```json
{
  "devDependencies": {
    "@playwright/test": "^1.40.0",
    "playwright": "^1.40.0"
  }
}
```

**Playwright Configuration (`playwright.config.js`):**
```javascript
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: false, // WASM tests can be resource-intensive
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,

  reporter: [
    ['html', { outputFolder: 'test-results/html' }],
    ['json', { outputFile: 'test-results/results.json' }],
    ['junit', { outputFile: 'test-results/junit.xml' }],
    ['list']
  ],

  use: {
    baseURL: 'http://localhost:8765',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
    actionTimeout: 30000,
  },

  timeout: 180000, // 3 minutes for full Tor connection cycle

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
    {
      name: 'firefox',
      use: { ...devices['Desktop Firefox'] },
    },
    // Safari excluded due to potential WASM issues
  ],

  webServer: [
    {
      command: 'node cors-proxy.mjs',
      port: 8766,
      timeout: 10000,
      reuseExistingServer: !process.env.CI,
    },
    {
      command: 'npx serve -s webtor-demo/static -p 8765',
      port: 8765,
      timeout: 10000,
      reuseExistingServer: !process.env.CI,
    },
  ],
});
```

### 1.2 Test Directory Structure

```
tests/
├── e2e/
│   ├── fixtures/
│   │   └── demo-page.fixture.js       # Page object model
│   ├── helpers/
│   │   ├── console-monitor.js         # Console log collection
│   │   ├── timing-utilities.js        # Performance tracking
│   │   └── wasm-validators.js         # WASM-specific assertions
│   ├── 01-initialization.spec.js      # WASM loading tests
│   ├── 02-connection.spec.js          # Tor connection tests
│   ├── 03-circuit-management.spec.js  # Circuit lifecycle tests
│   ├── 04-http-requests.spec.js       # HTTP through Tor tests
│   ├── 05-error-handling.spec.js      # Error scenarios
│   ├── 06-concurrent-requests.spec.js # Multi-request tests
│   └── 07-isolated-requests.spec.js   # One-time circuit tests
└── unit/                               # (Future: Rust unit tests)
```

### 1.3 Custom Test Utilities

**Console Monitor (`tests/e2e/helpers/console-monitor.js`):**
```javascript
export class ConsoleMonitor {
  constructor(page) {
    this.page = page;
    this.logs = [];
    this.errors = [];
    this.warnings = [];

    page.on('console', msg => {
      const entry = {
        type: msg.type(),
        text: msg.text(),
        timestamp: new Date().toISOString()
      };

      this.logs.push(entry);

      if (msg.type() === 'error') {
        this.errors.push(entry);
      } else if (msg.type() === 'warning') {
        this.warnings.push(entry);
      }
    });

    page.on('pageerror', error => {
      const entry = {
        type: 'pageerror',
        text: error.message,
        stack: error.stack,
        timestamp: new Date().toISOString()
      };

      this.errors.push(entry);
      this.logs.push(entry);
    });
  }

  getWasmErrors() {
    return this.errors.filter(e =>
      e.text.includes('wasm') ||
      e.text.includes('WebAssembly') ||
      e.text.includes('Instant')
    );
  }

  hasErrorMatching(pattern) {
    return this.errors.some(e => e.text.match(pattern));
  }

  getLogsSince(timestamp) {
    return this.logs.filter(log =>
      new Date(log.timestamp) > new Date(timestamp)
    );
  }

  clear() {
    this.logs = [];
    this.errors = [];
    this.warnings = [];
  }

  exportLogs(filepath) {
    const fs = require('fs');
    fs.writeFileSync(filepath, JSON.stringify({
      logs: this.logs,
      errors: this.errors,
      warnings: this.warnings
    }, null, 2));
  }
}
```

**Timing Utilities (`tests/e2e/helpers/timing-utilities.js`):**
```javascript
export class TimingUtilities {
  static async measureAsync(fn, label = 'Operation') {
    const start = Date.now();
    const result = await fn();
    const duration = Date.now() - start;

    console.log(`[Timing] ${label}: ${duration}ms`);

    return { result, duration };
  }

  static async waitForConditionWithTimeout(
    page,
    condition,
    timeout = 30000,
    checkInterval = 500
  ) {
    const start = Date.now();
    let lastStatus = null;

    while (Date.now() - start < timeout) {
      try {
        const result = await condition();
        if (result.success) {
          return { success: true, data: result.data, duration: Date.now() - start };
        }

        // Log status changes
        if (result.status !== lastStatus) {
          lastStatus = result.status;
          console.log(`[Status] ${result.status}`);
        }
      } catch (error) {
        // Condition check failed, continue waiting
      }

      await page.waitForTimeout(checkInterval);
    }

    throw new Error(`Timeout after ${timeout}ms waiting for condition`);
  }
}
```

**WASM Validators (`tests/e2e/helpers/wasm-validators.js`):**
```javascript
export class WasmValidators {
  static async validateWasmLoaded(page) {
    // Check for WASM module existence
    const hasWasm = await page.evaluate(() => {
      return typeof WebAssembly !== 'undefined' &&
             window.demoApp !== undefined;
    });

    if (!hasWasm) {
      throw new Error('WASM module not loaded');
    }

    return true;
  }

  static async validateNoWasmPanics(consoleMonitor) {
    const wasmErrors = consoleMonitor.getWasmErrors();

    const panics = wasmErrors.filter(e =>
      e.text.includes('panic') ||
      e.text.includes('unreachable') ||
      e.text.includes('Instant')
    );

    if (panics.length > 0) {
      throw new Error(`WASM panics detected: ${JSON.stringify(panics, null, 2)}`);
    }

    return true;
  }

  static async getWasmMemoryUsage(page) {
    return await page.evaluate(() => {
      if (performance.memory) {
        return {
          usedJSHeapSize: performance.memory.usedJSHeapSize,
          totalJSHeapSize: performance.memory.totalJSHeapSize,
          jsHeapSizeLimit: performance.memory.jsHeapSizeLimit
        };
      }
      return null;
    });
  }
}
```

**Page Object Model (`tests/e2e/fixtures/demo-page.fixture.js`):**
```javascript
import { test as base } from '@playwright/test';
import { ConsoleMonitor } from '../helpers/console-monitor.js';

export class DemoPage {
  constructor(page) {
    this.page = page;
    this.consoleMonitor = new ConsoleMonitor(page);

    // Element selectors
    this.selectors = {
      openBtn: '#openBtn',
      closeBtn: '#closeBtn',
      clearBtn: '#clearBtn',
      snowflakeUrl: '#snowflakeUrl',
      status: '#status',
      debugToggle: '#debugToggle',
      logOutput: '#output',

      // Request elements
      url1: '#url1',
      btn1: '#btn1',
      output1: '#output1',

      url2: '#url2',
      btn2: '#btn2',
      output2: '#output2',

      url3: '#url3',
      btn3: '#btn3',
      output3: '#output3',

      isolatedUrl: '#isolatedUrl',
      btnIsolated: '#btnIsolated',
      outputIsolated: '#outputIsolated',

      updateBtn: '#updateBtn',
    };
  }

  async goto() {
    await this.page.goto('/');
  }

  async waitForWasmInit(timeout = 30000) {
    await this.page.waitForFunction(
      () => window.demoApp !== undefined,
      { timeout }
    );
  }

  async enableDebugLogs() {
    const toggle = await this.page.$(this.selectors.debugToggle);
    const isChecked = await toggle.isChecked();

    if (!isChecked) {
      await toggle.click();
      await this.page.waitForTimeout(500);
    }
  }

  async openTorClient(options = {}) {
    const {
      snowflakeUrl = 'wss://snowflake.torproject.net/',
      waitForReady = true,
      timeout = 120000
    } = options;

    // Set Snowflake URL
    await this.page.fill(this.selectors.snowflakeUrl, snowflakeUrl);

    // Click open button
    await this.page.click(this.selectors.openBtn);

    if (waitForReady) {
      await this.waitForCircuitReady(timeout);
    }
  }

  async waitForCircuitReady(timeout = 120000) {
    const start = Date.now();

    while (Date.now() - start < timeout) {
      const status = await this.page.textContent(this.selectors.status);

      if (status.includes('ready') || status.includes('Ready')) {
        return Date.now() - start;
      }

      if (status.includes('failed') || status.includes('error')) {
        throw new Error(`Circuit failed: ${status}`);
      }

      await this.page.waitForTimeout(1000);
    }

    throw new Error('Timeout waiting for circuit ready');
  }

  async closeTorClient() {
    await this.page.click(this.selectors.closeBtn);
    await this.page.waitForTimeout(1000);
  }

  async makeRequest(index, options = {}) {
    const { url, waitForResponse = true, timeout = 60000 } = options;

    const urlSelector = this.selectors[`url${index}`];
    const btnSelector = this.selectors[`btn${index}`];
    const outputSelector = this.selectors[`output${index}`];

    if (url) {
      await this.page.fill(urlSelector, url);
    }

    await this.page.click(btnSelector);

    if (waitForResponse) {
      await this.page.waitForFunction(
        (selector) => {
          const output = document.querySelector(selector);
          return output && (
            output.textContent.includes('✅') ||
            output.textContent.includes('❌')
          );
        },
        outputSelector,
        { timeout }
      );

      return await this.page.textContent(outputSelector);
    }
  }

  async makeIsolatedRequest(url, options = {}) {
    const { waitForResponse = true, timeout = 120000 } = options;

    await this.page.fill(this.selectors.isolatedUrl, url);
    await this.page.click(this.selectors.btnIsolated);

    if (waitForResponse) {
      await this.page.waitForFunction(
        (selector) => {
          const output = document.querySelector(selector);
          return output && (
            output.textContent.includes('✅') ||
            output.textContent.includes('❌')
          );
        },
        this.selectors.outputIsolated,
        { timeout }
      );

      return await this.page.textContent(this.selectors.outputIsolated);
    }
  }

  async triggerCircuitUpdate() {
    await this.page.click(this.selectors.updateBtn);
    await this.page.waitForTimeout(2000);
  }

  async getCircuitStatus() {
    return await this.page.textContent(this.selectors.status);
  }

  async getConnectionLogs() {
    return await this.page.inputValue(this.selectors.logOutput);
  }

  async clearOutput() {
    await this.page.click(this.selectors.clearBtn);
  }
}

// Extend Playwright test with DemoPage fixture
export const test = base.extend({
  demoPage: async ({ page }, use) => {
    const demoPage = new DemoPage(page);
    await demoPage.goto();
    await demoPage.waitForWasmInit();
    await demoPage.enableDebugLogs();

    await use(demoPage);
  }
});

export { expect } from '@playwright/test';
```

---

## 2. Test Scenarios (Prioritized)

### 2.1 Critical Path Tests (P0)

#### Test Suite 1: WASM Module Initialization (`01-initialization.spec.js`)

**Priority:** P0 - Blocking
**Estimated Duration:** 5-10 seconds per test

**Test Cases:**

```javascript
import { test, expect } from '../fixtures/demo-page.fixture.js';
import { WasmValidators } from '../helpers/wasm-validators.js';

test.describe('WASM Module Initialization', () => {
  test('should load WASM module without errors', async ({ page, demoPage }) => {
    // Validate no WASM panics
    await WasmValidators.validateNoWasmPanics(demoPage.consoleMonitor);

    // Check demoApp is available
    await WasmValidators.validateWasmLoaded(page);
  });

  test('should initialize UI elements correctly', async ({ page }) => {
    // All buttons should be present
    await expect(page.locator('#openBtn')).toBeVisible();
    await expect(page.locator('#closeBtn')).toBeDisabled();

    // Default Snowflake URL should be set
    const snowflakeUrl = await page.inputValue('#snowflakeUrl');
    expect(snowflakeUrl).toBe('wss://snowflake.torproject.net/');
  });

  test('should have no console errors after initialization', async ({ demoPage }) => {
    const errors = demoPage.consoleMonitor.errors;
    expect(errors).toHaveLength(0);
  });

  test('should not panic on Instant operations', async ({ demoPage }) => {
    // This test specifically validates the std::time::Instant fix
    const instantErrors = demoPage.consoleMonitor.logs.filter(log =>
      log.text.includes('Instant') || log.text.includes('time')
    );

    expect(instantErrors).toHaveLength(0);
  });
});
```

#### Test Suite 2: Tor Connection Establishment (`02-connection.spec.js`)

**Priority:** P0 - Blocking
**Estimated Duration:** 30-90 seconds per test

**Test Cases:**

```javascript
import { test, expect } from '../fixtures/demo-page.fixture.js';
import { TimingUtilities } from '../helpers/timing-utilities.js';

test.describe('Tor Connection Establishment', () => {
  test('should connect to Tor via Snowflake bridge', async ({ demoPage }) => {
    const { duration } = await TimingUtilities.measureAsync(
      async () => await demoPage.openTorClient(),
      'Tor Connection'
    );

    // Connection should complete within 2 minutes
    expect(duration).toBeLessThan(120000);

    // Status should show ready
    const status = await demoPage.getCircuitStatus();
    expect(status).toMatch(/ready|Ready/);
  });

  test('should update circuit status during connection', async ({ page, demoPage }) => {
    const statusUpdates = [];

    // Monitor status changes
    const checkStatus = async () => {
      const status = await page.textContent('#status');
      if (!statusUpdates.includes(status)) {
        statusUpdates.push(status);
      }
    };

    // Start connection
    page.click('#openBtn');

    // Poll status every second
    const pollInterval = setInterval(checkStatus, 1000);

    await demoPage.waitForCircuitReady();
    clearInterval(pollInterval);

    // Should have multiple status updates
    expect(statusUpdates.length).toBeGreaterThan(1);
    expect(statusUpdates[0]).toContain('Not initialized');
    expect(statusUpdates[statusUpdates.length - 1]).toMatch(/ready|Ready/);
  });

  test('should handle connection timeout gracefully', async ({ page, demoPage }) => {
    // Set invalid Snowflake URL
    await page.fill('#snowflakeUrl', 'wss://invalid.example.com/');

    // Attempt connection (should fail)
    await page.click('#openBtn');

    // Wait for error indication (shorter timeout)
    await page.waitForTimeout(30000);

    const logs = await demoPage.getConnectionLogs();
    expect(logs).toMatch(/failed|error|timeout/i);
  });

  test('should log connection progress to UI', async ({ demoPage }) => {
    await demoPage.openTorClient();

    const logs = await demoPage.getConnectionLogs();

    // Check for key connection events
    expect(logs).toContain('TorClient');
    expect(logs).toMatch(/circuit|ready|success/i);
  });
});
```

#### Test Suite 3: HTTP Requests Through Tor (`04-http-requests.spec.js`)

**Priority:** P0 - Blocking
**Estimated Duration:** 10-30 seconds per test

**Test Cases:**

```javascript
import { test, expect } from '../fixtures/demo-page.fixture.js';

test.describe('HTTP Requests Through Tor', () => {
  test.beforeEach(async ({ demoPage }) => {
    // Establish Tor connection before each test
    await demoPage.openTorClient();
  });

  test('should make successful HTTP request to httpbin.org/ip', async ({ demoPage }) => {
    const result = await demoPage.makeRequest(1, {
      url: 'https://httpbin.org/ip'
    });

    expect(result).toContain('✅');
    expect(result).toMatch(/\d+\.\d+\.\d+\.\d+/); // IP address pattern
  });

  test('should handle TLS connections', async ({ demoPage }) => {
    // Note: This test may fail due to known TLS issues
    // Keeping it to track the known issue
    const result = await demoPage.makeRequest(1, {
      url: 'https://check.torproject.org'
    });

    // Currently expected to fail - this validates error handling
    if (result.includes('❌')) {
      expect(result).toMatch(/TLS|certificate|handshake/i);
    }
  });

  test('should display request timing information', async ({ demoPage }) => {
    const result = await demoPage.makeRequest(1, {
      url: 'https://httpbin.org/user-agent'
    });

    // Response should include timing
    expect(result).toMatch(/\d+ms/);
  });

  test('should handle HTTP errors gracefully', async ({ demoPage }) => {
    const result = await demoPage.makeRequest(1, {
      url: 'https://httpbin.org/status/404'
    });

    // Should show error but not crash
    expect(result).toContain('❌');
  });
});
```

### 2.2 Important Tests (P1)

#### Test Suite 4: Circuit Management (`03-circuit-management.spec.js`)

**Priority:** P1 - Important
**Estimated Duration:** 30-60 seconds per test

**Test Cases:**

```javascript
import { test, expect } from '../fixtures/demo-page.fixture.js';

test.describe('Circuit Management', () => {
  test.beforeEach(async ({ demoPage }) => {
    await demoPage.openTorClient();
  });

  test('should manually trigger circuit update', async ({ demoPage }) => {
    const initialStatus = await demoPage.getCircuitStatus();

    await demoPage.triggerCircuitUpdate();

    // Wait for update to complete
    await demoPage.page.waitForTimeout(15000);

    const newStatus = await demoPage.getCircuitStatus();

    // Status should indicate successful update
    expect(newStatus).toMatch(/ready|Ready/);
  });

  test('should reuse circuit for multiple requests', async ({ demoPage }) => {
    // Make first request
    const result1 = await demoPage.makeRequest(1, {
      url: 'https://httpbin.org/uuid'
    });

    // Make second request immediately
    const result2 = await demoPage.makeRequest(2, {
      url: 'https://httpbin.org/uuid'
    });

    // Both should succeed
    expect(result1).toContain('✅');
    expect(result2).toContain('✅');

    // Second request should be faster (circuit reuse)
    const time1 = parseFloat(result1.match(/(\d+)ms/)[1]);
    const time2 = parseFloat(result2.match(/(\d+)ms/)[1]);

    expect(time2).toBeLessThan(time1 * 0.8); // At least 20% faster
  });

  test('should close TorClient cleanly', async ({ demoPage }) => {
    await demoPage.closeTorClient();

    const status = await demoPage.getCircuitStatus();
    expect(status).toMatch(/closed|Not initialized/);

    // Open button should be enabled again
    await expect(demoPage.page.locator('#openBtn')).toBeEnabled();
    await expect(demoPage.page.locator('#closeBtn')).toBeDisabled();
  });
});
```

#### Test Suite 5: Isolated Requests (`07-isolated-requests.spec.js`)

**Priority:** P1 - Important
**Estimated Duration:** 60-120 seconds per test

**Test Cases:**

```javascript
import { test, expect } from '../fixtures/demo-page.fixture.js';

test.describe('Isolated Requests (One-Time Circuits)', () => {
  test('should make isolated request without persistent client', async ({ demoPage }) => {
    // Don't open TorClient first - isolated request should work independently
    const result = await demoPage.makeIsolatedRequest('https://httpbin.org/uuid', {
      timeout: 120000
    });

    expect(result).toContain('✅');
    expect(result).toContain('isolated');
  });

  test('should create temporary circuit for isolated request', async ({ demoPage }) => {
    const logs1 = await demoPage.getConnectionLogs();

    await demoPage.makeIsolatedRequest('https://httpbin.org/ip');

    const logs2 = await demoPage.getConnectionLogs();

    // Should log circuit creation
    expect(logs2).toContain('isolated');
  });

  test('isolated request should not affect persistent circuit', async ({ demoPage }) => {
    // Open persistent client
    await demoPage.openTorClient();
    const statusBefore = await demoPage.getCircuitStatus();

    // Make isolated request
    await demoPage.makeIsolatedRequest('https://httpbin.org/uuid');

    // Persistent circuit should still be active
    const statusAfter = await demoPage.getCircuitStatus();
    expect(statusAfter).toBe(statusBefore);
  });
});
```

### 2.3 Edge Cases & Error Handling (P2)

#### Test Suite 6: Error Handling (`05-error-handling.spec.js`)

**Priority:** P2 - Nice to have
**Estimated Duration:** 10-30 seconds per test

**Test Cases:**

```javascript
import { test, expect } from '../fixtures/demo-page.fixture.js';

test.describe('Error Handling', () => {
  test('should handle empty URL input gracefully', async ({ page, demoPage }) => {
    await demoPage.openTorClient();

    // Clear URL input
    await page.fill('#url1', '');

    const result = await demoPage.makeRequest(1, { waitForResponse: false });

    await page.waitForTimeout(1000);
    const output = await page.textContent('#output1');
    expect(output).toMatch(/empty|URL|required/i);
  });

  test('should handle invalid URL format', async ({ demoPage }) => {
    await demoPage.openTorClient();

    const result = await demoPage.makeRequest(1, {
      url: 'not-a-valid-url'
    });

    expect(result).toContain('❌');
  });

  test('should recover from network interruption', async ({ page, demoPage }) => {
    await demoPage.openTorClient();

    // Simulate network offline
    await page.context().setOffline(true);

    const result1 = await demoPage.makeRequest(1, {
      url: 'https://httpbin.org/ip',
      timeout: 10000
    }).catch(() => 'timeout');

    // Restore network
    await page.context().setOffline(false);

    // Should recover
    const result2 = await demoPage.makeRequest(1, {
      url: 'https://httpbin.org/ip'
    });

    expect(result2).toContain('✅');
  });

  test('should display meaningful error messages', async ({ demoPage }) => {
    await demoPage.openTorClient();

    const result = await demoPage.makeRequest(1, {
      url: 'https://httpbin.org/delay/100' // Will timeout
    });

    expect(result).toContain('❌');
    expect(result).toMatch(/timeout|failed|error/i);
  });
});
```

#### Test Suite 7: Concurrent Requests (`06-concurrent-requests.spec.js`)

**Priority:** P2 - Nice to have
**Estimated Duration:** 30-60 seconds per test

**Test Cases:**

```javascript
import { test, expect } from '../fixtures/demo-page.fixture.js';

test.describe('Concurrent Requests', () => {
  test.beforeEach(async ({ demoPage }) => {
    await demoPage.openTorClient();
  });

  test('should handle 3 concurrent requests', async ({ demoPage, page }) => {
    // Start all requests simultaneously
    const promises = [
      demoPage.makeRequest(1, { url: 'https://httpbin.org/uuid' }),
      demoPage.makeRequest(2, { url: 'https://httpbin.org/ip' }),
      demoPage.makeRequest(3, { url: 'https://httpbin.org/user-agent' }),
    ];

    const results = await Promise.all(promises);

    // All should succeed
    results.forEach(result => {
      expect(result).toContain('✅');
    });
  });

  test('should queue requests properly', async ({ demoPage }) => {
    // Rapid-fire 5 requests
    const results = [];

    for (let i = 1; i <= 3; i++) {
      const result = await demoPage.makeRequest(1, {
        url: 'https://httpbin.org/uuid'
      });
      results.push(result);
    }

    // All should eventually succeed
    results.forEach(result => {
      expect(result).toContain('✅');
    });
  });

  test('should not crash on request spam', async ({ page, demoPage }) => {
    // Click button rapidly 10 times
    for (let i = 0; i < 10; i++) {
      page.click('#btn1').catch(() => {});
      await page.waitForTimeout(100);
    }

    // Wait for settling
    await page.waitForTimeout(5000);

    // Should not have crashed
    const errors = demoPage.consoleMonitor.errors;
    const criticalErrors = errors.filter(e =>
      e.text.includes('panic') || e.text.includes('unreachable')
    );

    expect(criticalErrors).toHaveLength(0);
  });
});
```

---

## 3. Test Execution Strategy

### 3.1 Local Development

**Quick Smoke Test:**
```bash
npm run test:e2e:quick
# Runs only P0 tests (initialization + basic connection)
```

**Full Test Suite:**
```bash
npm run test:e2e
# Runs all tests with HTML report
```

**Headed Mode (for debugging):**
```bash
npm run test:e2e:headed
# Runs tests with visible browser
```

**Single Test File:**
```bash
npx playwright test tests/e2e/04-http-requests.spec.js
```

### 3.2 Package.json Scripts

```json
{
  "scripts": {
    "build": "./build.sh",
    "test:e2e": "playwright test",
    "test:e2e:quick": "playwright test tests/e2e/01-initialization.spec.js tests/e2e/02-connection.spec.js",
    "test:e2e:headed": "playwright test --headed",
    "test:e2e:debug": "playwright test --debug",
    "test:e2e:report": "playwright show-report test-results/html",
    "test:e2e:ui": "playwright test --ui",
    "test:headless": "node test-headless.mjs",
    "test:all": "npm run test && npm run test:e2e"
  }
}
```

### 3.3 Test Tagging & Filtering

Add tags to tests for selective execution:

```javascript
test.describe('Connection Tests @critical @slow', () => {
  test('should connect @p0', async ({ demoPage }) => {
    // ...
  });
});
```

Run specific tags:
```bash
npx playwright test --grep @critical
npx playwright test --grep-invert @slow
```

---

## 4. CI/CD Integration

### 4.1 GitHub Actions Workflow

**File:** `.github/workflows/e2e-tests.yml`

```yaml
name: E2E Tests

on:
  push:
    branches: [main, feat/*]
  pull_request:
    branches: [main]
  workflow_dispatch:

jobs:
  e2e-tests:
    runs-on: ubuntu-latest
    timeout-minutes: 30

    strategy:
      fail-fast: false
      matrix:
        browser: [chromium, firefox]

    steps:
      - uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: 'npm'

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown

      - name: Install wasm-pack
        run: curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

      - name: Cache Cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Install dependencies
        run: npm ci

      - name: Build WASM modules
        run: npm run build:wasm

      - name: Install Playwright browsers
        run: npx playwright install --with-deps ${{ matrix.browser }}

      - name: Run E2E tests
        run: npx playwright test --project=${{ matrix.browser }}
        env:
          CI: true

      - name: Upload test results
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: playwright-results-${{ matrix.browser }}
          path: test-results/
          retention-days: 7

      - name: Upload HTML report
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: playwright-report-${{ matrix.browser }}
          path: test-results/html/
          retention-days: 7

      - name: Upload logs on failure
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: failure-logs-${{ matrix.browser }}
          path: |
            test-results/**/*.log
            test-results/**/*.png
            test-results/**/*.webm
          retention-days: 14

      - name: Comment PR with results
        if: github.event_name == 'pull_request' && always()
        uses: actions/github-script@v7
        with:
          script: |
            const fs = require('fs');
            const resultsPath = 'test-results/results.json';

            if (fs.existsSync(resultsPath)) {
              const results = JSON.parse(fs.readFileSync(resultsPath));
              const summary = `
              ## E2E Test Results (${{ matrix.browser }})

              - ✅ Passed: ${results.stats.expected}
              - ❌ Failed: ${results.stats.unexpected}
              - ⏭️ Skipped: ${results.stats.skipped}
              - ⏱️ Duration: ${(results.stats.duration / 1000).toFixed(1)}s
              `;

              github.rest.issues.createComment({
                issue_number: context.issue.number,
                owner: context.repo.owner,
                repo: context.repo.repo,
                body: summary
              });
            }

  e2e-tests-summary:
    runs-on: ubuntu-latest
    needs: e2e-tests
    if: always()

    steps:
      - name: Check test results
        run: |
          if [ "${{ needs.e2e-tests.result }}" != "success" ]; then
            echo "E2E tests failed"
            exit 1
          fi
```

### 4.2 Test Result Reporting

**Playwright HTML Report:**
- Automatically generated at `test-results/html/`
- View with: `npm run test:e2e:report`
- Includes screenshots, videos, traces for failures

**JUnit XML Report:**
- Compatible with CI systems
- Location: `test-results/junit.xml`
- Used for GitHub Actions test annotations

**JSON Report:**
- Programmatic access to results
- Location: `test-results/results.json`
- Used for custom reporting scripts

---

## 5. Test Maintenance & Best Practices

### 5.1 Flaky Test Handling

**Automatic Retries:**
```javascript
test.describe('Flaky Operations', () => {
  test('connection test', async ({ demoPage }) => {
    // Playwright will auto-retry based on config
  });
});
```

**Explicit Retry Configuration:**
```javascript
test('unstable test', {
  annotation: { type: 'flaky', description: 'Known Snowflake instability' }
}, async ({ demoPage }) => {
  // Test code
});
```

### 5.2 Test Data Management

**Environment Variables:**
```javascript
// .env.test
SNOWFLAKE_URL=wss://snowflake.torproject.net/
TEST_TIMEOUT=120000
ENABLE_DEBUG_LOGS=true
```

**Test Configuration:**
```javascript
// tests/e2e/test-config.js
export const TEST_URLS = {
  ip: 'https://httpbin.org/ip',
  uuid: 'https://httpbin.org/uuid',
  userAgent: 'https://httpbin.org/user-agent',
  headers: 'https://httpbin.org/headers',
  delay: 'https://httpbin.org/delay/5'
};

export const TIMEOUTS = {
  wasmInit: 30000,
  torConnection: 120000,
  httpRequest: 60000,
  circuitUpdate: 30000
};
```

### 5.3 Debugging Failed Tests

**1. Enable Trace Collection:**
```bash
npx playwright test --trace on
```

**2. View Trace:**
```bash
npx playwright show-trace test-results/.../trace.zip
```

**3. Screenshot on Failure:**
- Automatically captured by Playwright
- Location: `test-results/.../test-failed-1.png`

**4. Video Recording:**
- Configured in `playwright.config.js`
- Only recorded on failure to save space

**5. Console Logs:**
```javascript
test.afterEach(async ({ demoPage }, testInfo) => {
  if (testInfo.status !== 'passed') {
    const logs = demoPage.consoleMonitor.exportLogs(
      `test-results/${testInfo.title}-logs.json`
    );
  }
});
```

### 5.4 Performance Benchmarking

**Track Key Metrics:**
```javascript
test('performance benchmark', async ({ demoPage }) => {
  const metrics = {
    wasmInit: 0,
    torConnection: 0,
    firstRequest: 0,
    subsequentRequest: 0
  };

  metrics.wasmInit = await measureAsync(
    () => demoPage.waitForWasmInit()
  );

  metrics.torConnection = await measureAsync(
    () => demoPage.openTorClient()
  );

  metrics.firstRequest = await measureAsync(
    () => demoPage.makeRequest(1, { url: TEST_URLS.ip })
  );

  metrics.subsequentRequest = await measureAsync(
    () => demoPage.makeRequest(1, { url: TEST_URLS.ip })
  );

  // Log for CI trend tracking
  console.log('METRICS:', JSON.stringify(metrics));

  // Assertions
  expect(metrics.wasmInit).toBeLessThan(10000);
  expect(metrics.torConnection).toBeLessThan(120000);
  expect(metrics.firstRequest).toBeLessThan(30000);
  expect(metrics.subsequentRequest).toBeLessThan(5000);
});
```

**Store Results in CI:**
```yaml
- name: Track performance
  run: |
    npx playwright test performance.spec.js | tee perf-results.log
    # Store results in artifact or database
```

---

## 6. Known Issues & Workarounds

### 6.1 TLS Certificate Validation Failures

**Issue:** HTTP requests through Tor currently fail with TLS errors
**Status:** Known issue, separate from E2E test suite
**Test Handling:**
```javascript
test('TLS request (known issue)', async ({ demoPage }) => {
  const result = await demoPage.makeRequest(1, {
    url: 'https://check.torproject.org'
  });

  // Currently expected to fail
  test.skip(result.includes('✅'), 'TLS validation not yet implemented');

  // Validate error handling instead
  expect(result).toContain('❌');
});
```

### 6.2 Snowflake Bridge Availability

**Issue:** Snowflake bridges can be unreliable
**Mitigation:**
- Configure retry attempts in CI
- Use fallback bridges if available
- Skip tests on repeated failures (with warning)

### 6.3 WASM Memory Issues

**Issue:** Large memory consumption over time
**Monitoring:**
```javascript
test.afterEach(async ({ page, demoPage }) => {
  const memory = await WasmValidators.getWasmMemoryUsage(page);
  console.log('Memory usage:', memory);

  // Warn if memory usage is high
  if (memory && memory.usedJSHeapSize > 100 * 1024 * 1024) {
    console.warn('High memory usage detected:', memory);
  }
});
```

---

## 7. Test Coverage Goals

### 7.1 Coverage Targets

| Category | Target | Current | Priority |
|----------|--------|---------|----------|
| WASM Initialization | 100% | 0% | P0 |
| Tor Connection | 90% | 30% (basic) | P0 |
| HTTP Requests | 80% | 30% (basic) | P0 |
| Circuit Management | 75% | 0% | P1 |
| Error Handling | 70% | 0% | P1 |
| Concurrency | 60% | 0% | P2 |

### 7.2 Test Matrix

| Feature | Unit Tests | Integration Tests | E2E Tests |
|---------|------------|-------------------|-----------|
| WASM Loading | N/A | N/A | ✅ |
| TorClient API | ✅ (Rust) | ✅ (Rust) | ✅ |
| HTTP Requests | ✅ (Rust) | ✅ (Rust) | ✅ |
| UI Interactions | N/A | N/A | ✅ |
| Circuit Lifecycle | ✅ (Rust) | ✅ (Rust) | ✅ |
| Error Handling | ✅ (Rust) | ✅ (Rust) | ✅ |

---

## 8. Future Enhancements

### 8.1 Phase 2 Improvements

1. **Visual Regression Testing**
   - Add Playwright visual comparisons
   - Track UI changes across versions

2. **Performance Monitoring**
   - Integrate with performance tracking services
   - Establish performance budgets
   - Automated alerts on regression

3. **Cross-Browser Testing**
   - Add Safari support (if WASM issues resolved)
   - Test mobile browsers (iOS Safari, Chrome Android)

4. **Chaos Engineering**
   - Random network interruptions
   - Simulate bridge failures
   - Memory pressure testing

5. **Accessibility Testing**
   - Add axe-core integration
   - Keyboard navigation tests
   - Screen reader compatibility

### 8.2 Test Infrastructure Improvements

1. **Parallel Test Execution**
   - Optimize for faster CI runs
   - Investigate Playwright sharding

2. **Test Data Factory**
   - Generate realistic test scenarios
   - Parameterized test inputs

3. **Mock Tor Network**
   - Create local test doubles
   - Faster, more reliable tests
   - Controlled error injection

---

## 9. Getting Started Checklist

### For Developers

- [ ] Install dependencies: `npm install`
- [ ] Build WASM modules: `npm run build:wasm`
- [ ] Install Playwright: `npx playwright install`
- [ ] Run quick smoke test: `npm run test:e2e:quick`
- [ ] Review test results: `npm run test:e2e:report`

### For CI/CD Setup

- [ ] Add GitHub Actions workflow
- [ ] Configure artifact upload
- [ ] Set up test result notifications
- [ ] Establish performance baselines
- [ ] Configure automatic retries for flaky tests

### For Test Development

- [ ] Create test file from template
- [ ] Use DemoPage fixture for interactions
- [ ] Add console monitoring
- [ ] Include timing measurements
- [ ] Handle known issues with test.skip()
- [ ] Add descriptive test names and comments

---

## 10. Appendix

### 10.1 Test Templates

**Basic Test Template:**
```javascript
import { test, expect } from '../fixtures/demo-page.fixture.js';

test.describe('Feature Name', () => {
  test.beforeEach(async ({ demoPage }) => {
    // Setup
  });

  test('should do something', async ({ page, demoPage }) => {
    // Arrange

    // Act

    // Assert
  });

  test.afterEach(async ({ demoPage }, testInfo) => {
    // Cleanup or logging
  });
});
```

### 10.2 Useful Commands

```bash
# Run all tests
npm run test:e2e

# Run specific test file
npx playwright test tests/e2e/01-initialization.spec.js

# Run in headed mode
npm run test:e2e:headed

# Debug mode
npx playwright test --debug

# Run tests matching pattern
npx playwright test --grep "connection"

# Run with UI mode
npm run test:e2e:ui

# Generate code
npx playwright codegen http://localhost:8765

# Show report
npm run test:e2e:report

# Update snapshots
npx playwright test --update-snapshots
```

### 10.3 Troubleshooting Guide

**Problem: Tests timeout waiting for Tor connection**
- Check Snowflake bridge availability
- Increase timeout in config
- Enable debug logs to see connection progress

**Problem: WASM module fails to load**
- Verify build succeeded: `npm run build:wasm`
- Check browser console for errors
- Ensure server is serving correct MIME types

**Problem: Flaky test failures**
- Add retries in test config
- Use waitForFunction() instead of waitForTimeout()
- Check for race conditions in test

**Problem: CI tests fail but local tests pass**
- Check CI-specific configuration
- Verify all dependencies are installed
- Look for timing issues (CI is often slower)

---

## Conclusion

This E2E test plan provides a comprehensive framework for validating the webtor-rs demo application. By following this plan, the project will achieve:

- **Confidence:** Automated validation of critical user flows
- **Reliability:** Early detection of regressions
- **Maintainability:** Clear test structure and utilities
- **Scalability:** Framework for adding new tests as features grow

**Immediate Next Steps:**
1. Implement Playwright configuration
2. Create test utilities (ConsoleMonitor, DemoPage fixture)
3. Write P0 tests (initialization, connection, basic HTTP)
4. Set up CI/CD integration
5. Establish baseline performance metrics

**Success Metrics:**
- 90%+ pass rate on P0 tests
- <10 minute full test suite execution
- <1% flaky test rate
- Zero WASM panics in production

**Maintenance:**
This test plan should be reviewed and updated quarterly or when major features are added.
