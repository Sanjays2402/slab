import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "tests/website",
  timeout: 45000,
  workers: 1,
  use: {
    baseURL: process.env.SLAB_SITE_URL || "https://sanjays2402.github.io/slab/",
    headless: true,
    trace: "retain-on-failure",
    launchOptions: process.env.SLAB_CHROMIUM_PATH ? { executablePath: process.env.SLAB_CHROMIUM_PATH } : {},
  },
});
