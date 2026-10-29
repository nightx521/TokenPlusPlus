import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { setTimeout as pause } from 'node:timers/promises';
import { mkdir } from 'node:fs/promises';
import assert from 'node:assert/strict';
const executable = process.argv[2];
if (!executable) throw new Error('Pass desktop executable path');
const child = spawn(executable, [], { env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9237 --remote-debugging-address=127.0.0.1' }, stdio: 'ignore' });
let browser;
try {
  for (let i = 0; i < 50; i++) {
    try { browser = await chromium.connectOverCDP('http://127.0.0.1:9237'); break; }
    catch { await pause(400); }
  }
  if (!browser) throw new Error('Cannot inspect application WebView');
  const context = browser.contexts()[0];
  let page;
  for (let i = 0; i < 30; i++) { page = context.pages().find(p => p.url().includes('tauri.localhost')); if (page) break; await pause(300); }
  if (!page) throw new Error('Application page not found');
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.getByRole('heading', { name: '额度，一眼看清。' }).waitFor();
  await page.locator('.provider.codex .badge.ok').waitFor({ timeout: 45000 });
  assert.equal(await page.locator('.provider.codex .progress-track').count(), 2);
  const pin = page.getByRole('button', { name: '窗口置顶', exact: true });
  const initial = await pin.getAttribute('aria-pressed');
  await pin.click();
  await page.waitForFunction(expected => document.querySelector('[aria-label="窗口置顶"]')?.getAttribute('aria-pressed') === expected, initial === 'true' ? 'false' : 'true');
  await pin.click();
  await page.waitForFunction(expected => document.querySelector('[aria-label="窗口置顶"]')?.getAttribute('aria-pressed') === expected, initial);
  const dismiss = page.getByRole('button', { name: '关闭提示', exact: true });
  if (await dismiss.isVisible()) await dismiss.click();
  await mkdir('.qa', { recursive: true });
  await page.screenshot({ path: '.qa/desktop-live.png' });
  await page.getByRole('button', { name: '体验演示' }).click();
  await page.getByText('以下为演示数据，用于体验界面').waitFor();
  await page.screenshot({ path: '.qa/desktop-demo.png' });
  await page.getByRole('button', { name: '退出演示', exact: true }).first().click();
  await page.getByRole('button', { name: '打开设置' }).click();
  await page.getByRole('button', { name: '华为云', exact: true }).click();
  assert.equal(await page.getByLabel('账号站点').inputValue(), 'cn');
  await page.screenshot({ path: '.qa/desktop-settings.png' });
  await page.getByRole('button', { name: '返回额度一览' }).click();
  // Check the app's own hide command without leaving a debugging instance running.
  await page.getByRole('button', { name: '收起到托盘' }).click();
  await page.waitForFunction(async () => !(await window.__TAURI_INTERNALS__.invoke('plugin:window|is_visible', { label: 'main' })));
  assert.deepEqual(errors, []);
  console.log('Desktop check passed: native IPC, two live Codex windows, pin persistence, demo, settings, hide command.');
} finally {
  if (browser) await browser.close();
  child.kill();
}
