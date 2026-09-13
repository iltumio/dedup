import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  timeout: 30000,
  use: {
    baseURL: "http://127.0.0.1:5188",
    viewport: { width: 1100, height: 700 },
    launchOptions: {
      executablePath:
        process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH || undefined,
    },
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  webServer: {
    command: "pnpm dev --host 127.0.0.1 --port 5188",
    url: "http://127.0.0.1:5188",
    reuseExistingServer: !process.env.CI,
  },
});
