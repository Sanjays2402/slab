import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "tests/e2e",
  timeout: 45000,
  workers: 1,
  use: {
    baseURL: "http://127.0.0.1:1420",
    headless: true,
    trace: "retain-on-failure",
    launchOptions: process.env.SLAB_CHROMIUM_PATH ? { executablePath: process.env.SLAB_CHROMIUM_PATH } : {},
  },
  webServer: {
    command: "node node_modules/vite/bin/vite.js --host 127.0.0.1",
    url: "http://127.0.0.1:1420",
    reuseExistingServer: !process.env.CI,
  },
});
