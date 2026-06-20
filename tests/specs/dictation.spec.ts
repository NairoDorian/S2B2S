import { test, expect } from "@playwright/test";
import { mockTauriIpc, emitMockEvent } from "../helpers/tauri-mock";

test.describe("Dictation HUD Overlay", () => {
  test("shows overlay and responds to status events", async ({ page }) => {
    // 1. Mock Tauri IPC layer and override cancel operation in browser context
    await mockTauriIpc(page);
    await page.addInitScript(() => {
      (window as any).__mockHandlers = {
        ...(window as any).__mockHandlers,
        cancel_operation: () => {
          (window as any).__cancelCalled = true;
          return null;
        },
      };
    });

    // 2. Open the overlay page
    await page.goto("/src/overlay/index.html");

    // 3. Emit show-overlay with recording state
    await emitMockEvent(page, "show-overlay", "recording");

    // 4. Assert overlay is visible and in recording state
    const overlay = page.locator(".recording-overlay");
    await expect(overlay).toHaveClass(/fade-in/);
    
    // 5. Emit mic-levels to verify VAD bars rendering
    await emitMockEvent(page, "mic-level", [0.1, 0.3, 0.8, 0.9, 0.5, 0.2, 0.1, 0.0, 0.4]);
    const bar = page.locator(".bar").first();
    await expect(bar).toBeVisible();

    // 6. Click the cancel button and verify the Tauri command is called
    const cancelBtn = page.locator(".cancel-button");
    await expect(cancelBtn).toBeVisible();
    await cancelBtn.click();
    
    const wasCancelCalled = await page.evaluate(() => (window as any).__cancelCalled);
    expect(wasCancelCalled).toBe(true);

    // 7. Test state transition to transcribing
    await emitMockEvent(page, "show-overlay", "transcribing");
    const transcribingText = page.locator(".transcribing-text");
    await expect(transcribingText).toBeVisible();

    // 8. Test state transition to speaking
    await emitMockEvent(page, "show-overlay", "speaking");
    await expect(transcribingText).toBeVisible();

    // 9. Emit hide-overlay and verify hidden class
    await emitMockEvent(page, "hide-overlay", null);
    await expect(overlay).not.toHaveClass(/fade-in/);
  });
});
