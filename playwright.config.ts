import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: [["html", { open: "never" }]],
  use: {
    baseURL: "http://localhost:1430",
    trace: "on-first-retry",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    // Keep browser tests off Tauri's fixed development port. Launch Vite
    // directly so teardown does not leave a bunx child holding the listener.
    command: "bun node_modules/vite/bin/vite.js --port 1430",
    url: "http://localhost:1430",
    reuseExistingServer: !process.env.CI,
    timeout: 30000,
  },
});
