const { chromium } = require('playwright');

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });

  await page.goto('http://127.0.0.1:5173/');
  await page.waitForTimeout(1000);

  await page.fill('input#username', 'testuser');
  await page.fill('input#password', 'anypass');
  await page.click('button[type="submit"]');

  await page.waitForTimeout(2500);
  await page.screenshot({ path: '/tmp/workspace-after-login.png', fullPage: false });

  await browser.close();
})();
