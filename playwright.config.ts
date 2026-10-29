import { defineConfig } from '@playwright/test';
export default defineConfig({ testDir: './tests', workers: 1, use: { browserName: 'chromium', channel: 'msedge', viewport: { width: 800, height: 820 } }, webServer: { command: 'npm run dev', url: 'http://127.0.0.1:1420', reuseExistingServer: true }, reporter: 'list' });
