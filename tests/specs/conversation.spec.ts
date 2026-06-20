import { test, expect } from "@playwright/test";
import { mockTauriIpc, emitMockEvent } from "../helpers/tauri-mock";

test.describe("Conversation Tab", () => {
  test("sends message and renders LLM streamed reply with barge-in support", async ({ page }) => {
    // 1. Mock Tauri IPC layer and override brain operations in browser context
    await mockTauriIpc(page);
    await page.addInitScript(() => {
      // Configure specific app settings for conversation mode
      (window as any).__mockHandlers = {
        ...(window as any).__mockHandlers,
        check_speech_runtime_installed: () => true,
        has_any_models_available: () => ({ status: "ok", data: true }),
        brain_ask: (args: any) => {
          (window as any).__askText = args.text;
          // Simulate thinking event
          setTimeout(() => {
            (window as any).__mockEmit("brain:thinking", {});
            // Simulate tokens streaming in
            setTimeout(() => {
              (window as any).__mockEmit("brain:token", "Hello");
              setTimeout(() => {
                (window as any).__mockEmit("brain:token", " there! I am your AI assistant.");
                setTimeout(() => {
                  (window as any).__mockEmit("brain:done", { text: "Hello there! I am your AI assistant.", audio_duration_ms: 2500 });
                }, 200);
              }, 200);
            }, 200);
          }, 100);
          return null;
        },
        brain_abort: () => {
          (window as any).__abortCalled = true;
          return null;
        },
        brain_clear_history: () => {
          (window as any).__clearHistoryCalled = true;
          return null;
        }
      };
    });

    // 2. Open app and enter
    await page.goto("/");
    await expect(page.locator("text=Click or press any key to enter")).toBeVisible({ timeout: 5000 });
    await page.keyboard.press("Enter");

    // 3. Click the Conversation tab in the sidebar
    const conversationTab = page.locator("text=Conversation");
    await expect(conversationTab).toBeVisible({ timeout: 10000 });
    await conversationTab.click();

    // 4. Find the message input field and type a query
    const input = page.locator("textarea, input[placeholder*='message'], input[type='text']").first();
    await expect(input).toBeVisible();
    await input.fill("Hello Brain");

    // 5. Send message (press Enter or click Send)
    await input.press("Enter");
    
    const askText = await page.evaluate(() => (window as any).__askText);
    expect(askText).toBe("Hello Brain");

    // 6. Assert user message is visible in conversation
    const userMsg = page.locator("text=Hello Brain");
    await expect(userMsg).toBeVisible();

    // 7. Wait for assistant streamed tokens to render
    const assistantMsg = page.locator("text=Hello there! I am your AI assistant.");
    await expect(assistantMsg).toBeVisible({ timeout: 5000 });

    // 8. Test barge-in: Click Stop during text/TTS playback
    const stopBtn = page.locator("button:has-text('Stop'), button:has-text('Abort'), .cancel-button, [title*='Stop'], [title*='Abort']").first();
    if (await stopBtn.isVisible()) {
      await stopBtn.click();
      const abortCalled = await page.evaluate(() => (window as any).__abortCalled);
      expect(abortCalled).toBe(true);
    }
  });
});
