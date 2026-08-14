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
    const overlay = page.locator(".ov-stage");
    await expect(overlay).toBeVisible();
    await expect(overlay).toHaveClass(/ov-fade show/);

    // 5. Emit mic-levels to verify VAD bars rendering
    await emitMockEvent(
      page,
      "mic-level",
      [0.1, 0.3, 0.8, 0.9, 0.5, 0.2, 0.1, 0.0, 0.4],
    );
    await expect(page.locator(".swave")).toBeVisible();

    // 6. Click the cancel button and verify the Tauri command is called
    const cancelBtn = page.locator("button[aria-label='cancel']");
    await expect(cancelBtn).toBeVisible();
    await cancelBtn.click();

    const wasCancelCalled = await page.evaluate(
      () => (window as any).__cancelCalled,
    );
    expect(wasCancelCalled).toBe(true);

    // 7. Test state transition to transcribing (working row with label)
    await emitMockEvent(page, "show-overlay", "transcribing");
    await expect(page.locator(".swork-label")).toBeVisible();

    // 8. Test state transition to speaking (listening row with waveform)
    await emitMockEvent(page, "show-overlay", "speaking");
    await expect(page.locator(".swave")).toBeVisible();

    // 9. Emit hide-overlay and verify the stage unmounts
    await emitMockEvent(page, "hide-overlay", null);
    await expect(page.locator(".ov-stage")).toHaveCount(0);
  });
});
