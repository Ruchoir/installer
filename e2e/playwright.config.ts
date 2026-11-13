// The end-to-end test of a development instance (`ruchoir-install dev`), run in CI once it is up.
// Playwright is an accepted exception, for tests only (Ruchoir/.github, dependency-exceptions.md).
import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "tests",
  timeout: 60_000,
  retries: 0,
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: process.env.RUCHOIR_URL ?? "https://id.localhost",
    // The instance's certificate comes from its own development authority.
    ignoreHTTPSErrors: true,
    trace: "retain-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
