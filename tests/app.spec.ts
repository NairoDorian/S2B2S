import { test, expect } from "@playwright/test";
import { mockTauriIpc } from "./helpers/tauri-mock";

test.describe("s2b2s App", () => {
  test("dev server responds", async ({ page }) => {
    await mockTauriIpc(page);
    // Just verify the dev server is running and responds
    const response = await page.goto("/");
    expect(response?.status()).toBe(200);
  });

  test("page has html structure", async ({ page }) => {
    await mockTauriIpc(page);
    await page.goto("/");

    // Verify basic HTML structure exists
    const html = await page.content();
    expect(html).toContain("<html");
    expect(html).toContain("<body");
  });
});
