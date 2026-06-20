import { test, expect } from "@playwright/test";
import { mockTauriIpc } from "../helpers/tauri-mock";

test.describe("Onboarding Flow", () => {
  test("runs through the complete onboarding flow successfully", async ({
    page,
  }) => {
    // 1. Mock Tauri IPC layer and override check commands for a fresh installation
    await mockTauriIpc(page);
    await page.addInitScript(() => {
      (window as any).__mockHandlers = {
        ...(window as any).__mockHandlers,
        check_speech_runtime_installed: () => false,
        has_any_models_available: () => false,
      };
    });

    // 2. Open app
    await page.goto("/");

    // 3. Dismiss the loading screen to enter
    await expect(
      page.locator("text=Click or press any key to enter"),
    ).toBeVisible({ timeout: 5000 });
    await page.keyboard.press("Enter");

    // 4. Assert Speech Runtime setup screen is displayed
    const setupTitle = page.locator("text=Setup Speech Runtime");
    await expect(setupTitle).toBeVisible({ timeout: 10000 });

    // 5. Trigger Speech Runtime installation
    const installBtn = page.locator(
      "button:has-text('Install Speech Runtime')",
    );
    await expect(installBtn).toBeVisible();
    await installBtn.click();

    // 6. Assert transition to Installing state
    const installingTitle = page.locator("text=Installing Speech Runtime...");
    await expect(installingTitle).toBeVisible();

    // 7. Wait for automatic success transition (mocked in tauri-mock.ts) to Model Selection
    const modelSelectionTitle = page.locator(
      "text=To get started, choose a transcription model",
    );
    await expect(modelSelectionTitle).toBeVisible({ timeout: 10000 });

    // 8. Trigger model download
    const downloadBtn = page
      .locator("role=button[name*='Parakeet V3']")
      .first();
    await expect(downloadBtn).toBeVisible();
    await downloadBtn.click();

    // 9. Assert progress indicator is shown
    const downloadingLabel = page.locator("text=Downloading");
    await expect(downloadingLabel).toBeVisible();

    // 10. Wait for automatic downloaded status and transition to Main App
    const sidebar = page.locator(".flex-1.flex.overflow-hidden").first();
    await expect(sidebar).toBeVisible({ timeout: 10000 });
  });
});
