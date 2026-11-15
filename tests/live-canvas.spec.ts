import { test, expect } from "./harness/fixtures";

test.use({ settingsOverrides: { ui_scale: 1 }, reducedMotion: "reduce" });

test("canvas controls, live text reveal and native window lifecycle", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.getByRole("navigation")).toBeVisible();
  await page.evaluate(() => {
    const w = window as any;
    const original = w.__TAURI_INTERNALS__.invoke;
    w.__TAURI_INTERNALS__.invoke = async (cmd: string, args: any) => {
      if (cmd === "set_live_canvas_active") {
        w.__TAURI_CALLS__.push({ cmd, args });
        return null;
      }
      if (cmd === "live_mode_start" || cmd === "live_mode_stop") {
        w.__TAURI_CALLS__.push({ cmd, args });
        const status = await original("live_mode_status", {});
        await original("plugin:event|emit", {
          event: "live-mode-state-event",
          payload: {
            status: {
              ...status,
              phase: cmd === "live_mode_start" ? "listening" : "idle",
            },
          },
        });
        return null;
      }
      if (cmd === "overlay_scope_frame") return new ArrayBuffer(32);
      return original(cmd, args);
    };
  });
  await page
    .getByRole("button", { name: "Open Live Canvas", exact: true })
    .click();
  const telemetry = page.getByRole("button", { name: "Telemetry" });
  await expect(telemetry).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Copy", exact: true }),
  ).toHaveCount(0);
  await telemetry.click();
  await expect(telemetry).toHaveAttribute("aria-expanded", "true");
  await telemetry.click();
  await expect(telemetry).toHaveAttribute("aria-expanded", "false");
  await page
    .getByRole("button", { name: "Start listening", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Stop", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".live-canvas-microphone svg")).toHaveCount(0);
  await page.evaluate(() =>
    (window as any).__TAURI_INTERNALS__.invoke("plugin:event|emit", {
      event: "stream-text-event",
      payload: {
        committed: "",
        tentative: "A",
        slot: null,
      },
    }),
  );
  await expect(page.locator("main")).toHaveText("A");
  await expect
    .poll(() =>
      page
        .locator(".live-canvas-particle")
        .evaluate((el) => el.getAnimations().length),
    )
    .toBe(0);
  const text = "A quiet canvas for a clear thought.";
  await page.evaluate(
    (live) =>
      (window as any).__TAURI_INTERNALS__.invoke("plugin:event|emit", {
        event: "stream-text-event",
        payload: { committed: "", tentative: live, slot: null },
      }),
    text,
  );
  // The first paint is a partial hypothesis; the whole sentence follows.
  await expect.poll(() => page.locator("main").innerText()).not.toContain(text);
  await expect
    .poll(() =>
      page
        .locator(".live-canvas-particle")
        .evaluate((el) => el.getAnimations().length),
    )
    .toBe(1);
  const flight = await page.locator(".live-canvas-particle").evaluate((el) => {
    const animation = el.getAnimations()[0];
    animation.pause();
    animation.currentTime = 40;
    const glyphAnimation = document
      .querySelector(".live-canvas-arriving-letter")!
      .getAnimations()[0];
    glyphAnimation.pause();
    glyphAnimation.currentTime = 40;
    const frames = (animation.effect as KeyframeEffect).getKeyframes();
    const source = document
      .querySelector(".live-canvas-analyzer")!
      .getBoundingClientRect();
    const glyph = document
      .querySelector(".live-canvas-arriving-letter")!
      .getBoundingClientRect();
    return {
      start: frames[0].transform,
      end: frames.at(-1)!.transform,
      x0: source.x + source.width / 2 - 2,
      y0: source.y + source.height / 2 - 2,
      x1: glyph.x + glyph.width / 2 - 2,
      y1: glyph.y + glyph.height / 2 - 2,
    };
  });
  const start = String(flight.start).match(
    /translate\(([-\d.]+)px, ([-\d.]+)px\)/,
  )!;
  const end = String(flight.end).match(
    /translate\(([-\d.]+)px, ([-\d.]+)px\)/,
  )!;
  expect(Number(start[1])).toBeCloseTo(flight.x0, 2);
  expect(Number(start[2])).toBeCloseTo(flight.y0, 2);
  expect(Number(end[1])).toBeCloseTo(flight.x1, 2);
  expect(Number(end[2])).toBeCloseTo(flight.y1, 2);
  expect(flight.end).toContain("scale(2.4)");
  const visibleFlight = await page
    .locator(".live-canvas-particle")
    .evaluate((el) => {
      const animation = el.getAnimations()[0];
      animation.pause();
      animation.currentTime = 40;
      const opacity = Number(getComputedStyle(el).opacity);
      const glyph = document.querySelector(".live-canvas-arriving-letter")!;
      const fade = glyph.getAnimations()[0];
      fade.pause();
      fade.currentTime = 40;
      const glyphOpacity = Number(getComputedStyle(glyph).opacity);
      animation.play();
      fade.play();
      return { opacity, glyphOpacity };
    });
  expect(visibleFlight.opacity).toBe(1);
  expect(visibleFlight.glyphOpacity).toBe(0);
  await expect(page.locator("main")).toContainText(text);
  await page.evaluate(() =>
    (window as any).__TAURI_INTERNALS__.invoke("plugin:event|emit", {
      event: "stream-text-event",
      payload: { committed: "Extra model output", tentative: "", slot: 1 },
    }),
  );
  await expect(page.locator("main")).toContainText(text);
  await expect(page.locator("main")).not.toContainText("Extra model output");
  await expect
    .poll(() =>
      page
        .locator(".live-canvas-particle")
        .evaluate((el) => el.getAnimations().length),
    )
    .toBe(0);
  await page.evaluate(() => {
    (window as any).__CORRECTION_PARTICLES__ = 0;
    const animate = Element.prototype.animate;
    Element.prototype.animate = function (...args: Parameters<typeof animate>) {
      if (this.classList.contains("live-canvas-particle"))
        (window as any).__CORRECTION_PARTICLES__++;
      return animate.apply(this, args);
    };
  });
  await page.evaluate(() =>
    (window as any).__TAURI_INTERNALS__.invoke("plugin:event|emit", {
      event: "stream-text-event",
      payload: {
        committed: "A quiet canvas for a clear ",
        tentative: "idea.",
        slot: null,
      },
    }),
  );
  await expect(page.locator("[data-removing=true]")).toHaveCount(0);
  await expect(page.locator("main")).toContainText(
    "A quiet canvas for a clear idea.",
  );
  await page.evaluate(
    (text) =>
      (window as any).__TAURI_INTERNALS__.invoke("plugin:event|emit", {
        event: "stream-text-event",
        payload: { committed: text, tentative: "", slot: null },
      }),
    text,
  );
  await expect(page.locator("main")).toContainText(text);
  expect(
    await page.evaluate(() => (window as any).__CORRECTION_PARTICLES__),
  ).toBe(0);
  await telemetry.click();
  await expect(page.locator("#live-canvas-telemetry")).toContainText("WPM");
  await telemetry.click();
  const exit = page.getByRole("button", { name: "Close", exact: true });
  await expect(exit).toHaveCSS("opacity", "0");
  await exit.hover();
  await expect(exit).toHaveCSS("opacity", "1");
  await page.mouse.move(0, 0);
  await expect(exit).toHaveCSS("opacity", "0");
  await page.screenshot({
    path: "test-results/live-canvas-desktop.png",
    omitBackground: true,
  });
  await page.setViewportSize({ width: 1060, height: 720 });
  const screenshot = await page.screenshot({
    path: "test-results/live-canvas-minimum.png",
    omitBackground: true,
  });
  const geometry = await page.evaluate(() => {
    const scope = document
      .querySelector(".live-canvas-analyzer")!
      .getBoundingClientRect();
    const transcript = document
      .querySelector(".live-canvas-transcript")!
      .getBoundingClientRect();
    return {
      scope: (scope.y + scope.height / 2) / innerHeight,
      transcript: (transcript.y + transcript.height / 2) / innerHeight,
    };
  });
  expect(geometry.scope).toBeCloseTo(1 / 3, 2);
  expect(geometry.transcript).toBeCloseTo(2 / 3, 2);
  await expect(page.locator("main")).not.toContainText("READY");
  await expect(telemetry).toHaveText("");
  const alpha = await page.evaluate(async (base64) => {
    const image = new Image();
    image.src = `data:image/png;base64,${base64}`;
    await image.decode();
    const canvas = document.createElement("canvas");
    canvas.width = image.width;
    canvas.height = image.height;
    const ctx = canvas.getContext("2d")!;
    ctx.drawImage(image, 0, 0);
    return [
      ctx.getImageData(0, 360, 1, 1).data[3],
      ctx.getImageData(16, 360, 1, 1).data[3],
      ctx.getImageData(530, 360, 1, 1).data[3],
    ];
  }, screenshot.toString("base64"));
  expect(alpha[0]).toBeLessThan(5);
  expect(alpha[1]).toBeGreaterThan(60);
  expect(alpha[1]).toBeLessThan(200);
  expect(alpha[2]).toBe(255);
  await page.getByRole("button", { name: "Clear", exact: true }).click();
  await expect(page.locator("main")).not.toContainText(text);
  await expect
    .poll(() =>
      page
        .locator(".live-canvas-particle")
        .evaluate((el) => el.getAnimations().length),
    )
    .toBe(0);
  await expect(
    page.getByRole("button", { name: "Copy", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "Stop", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Start listening", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".live-canvas-microphone svg")).toHaveCount(1);
  await page.getByRole("button", { name: "Close", exact: true }).hover();
  await page.getByRole("button", { name: "Close", exact: true }).click();
  await expect(page.getByRole("navigation")).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() =>
        document.documentElement.hasAttribute("data-live-canvas"),
      ),
    )
    .toBe(false);
  await page
    .getByRole("button", { name: "Open Live Canvas", exact: true })
    .click();
  await expect(telemetry).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("navigation")).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as any).__TAURI_CALLS__
            .filter((call: any) => call.cmd === "set_live_canvas_active")
            .at(-1)?.args.active,
      ),
    )
    .toBe(false);
});

test("Live Canvas timing sliders persist independently and reset", async ({
  page,
}) => {
  await page.goto("/");
  await page
    .getByRole("navigation")
    .getByRole("button", { name: "Live Canvas", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Live Canvas", exact: true }),
  ).toBeVisible();
  const sliders = page.getByRole("slider");
  await expect(sliders).toHaveCount(7);
  await expect(sliders.nth(0)).toHaveValue("1");
  await expect(sliders.nth(1)).toHaveValue("96");
  await expect(sliders.nth(2)).toHaveValue("8");
  await expect(sliders.nth(3)).toHaveValue("8");
  await expect(sliders.nth(4)).toHaveValue("8");
  await expect(sliders.nth(5)).toHaveValue("8");
  await sliders.nth(4).focus();
  await page.keyboard.press("End");
  await expect(sliders.nth(4)).toHaveValue("200");
  await expect(sliders.nth(5)).toHaveValue("8");
  await expect(sliders.nth(6)).toHaveValue("1");
  await sliders.nth(6).focus();
  await page.keyboard.press("End");
  await expect(sliders.nth(6)).toHaveValue("4");
  await sliders.nth(0).focus();
  await page.keyboard.press("End");
  await expect(sliders.nth(0)).toHaveValue("4");
  await sliders.nth(2).focus();
  await page.keyboard.press("ArrowRight");
  await expect(sliders.nth(2)).toHaveValue("9");
  await expect(sliders.nth(1)).toHaveValue("96");
  await expect(sliders.nth(3)).toHaveValue("8");
  await page.reload();
  await page
    .getByRole("navigation")
    .getByRole("button", { name: "Live Canvas", exact: true })
    .click();
  await expect(sliders.nth(0)).toHaveValue("4");
  await expect(sliders.nth(2)).toHaveValue("9");
  await page.screenshot({ path: "test-results/live-canvas-settings.png" });
  await expect(sliders.nth(4)).toHaveValue("200");
  await page
    .getByRole("button", { name: "Reset animation", exact: true })
    .click();
  await expect(sliders.nth(0)).toHaveValue("1");
  await expect(sliders.nth(2)).toHaveValue("8");
  await expect(sliders.nth(4)).toHaveValue("8");
});
