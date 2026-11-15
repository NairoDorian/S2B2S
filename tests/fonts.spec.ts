import { test, expect } from "./harness/fixtures";
import type { Page } from "@playwright/test";

test.use({ settingsOverrides: { ui_scale: 1 }, reducedMotion: "reduce" });

async function openFonts(page: Page) {
  await page.goto("/");
  await page
    .getByRole("navigation")
    .getByRole("button", { name: "About", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Fonts", exact: true }),
  ).toBeVisible();
}

async function selectFont(page: Page, role: string, option: string) {
  const input = page.getByRole("combobox", { name: role, exact: true });
  await input.fill(option);
  await page.getByRole("option", { name: option, exact: true }).click();
  await expect(input).toBeEnabled();
}

test("all four bundled faces load locally without a system font install", async ({
  page,
}) => {
  const failures: string[] = [];
  page.on("requestfailed", (request) => failures.push(request.url()));
  await openFonts(page);
  const loaded = await page.evaluate(async () => {
    const faces = [
      '400 15px "Libron"',
      '700 15px "Libron"',
      'italic 400 15px "Libron"',
      'italic 700 15px "Libron"',
    ];
    return Promise.all(
      faces.map(async (face) => (await document.fonts.load(face)).length),
    );
  });
  expect(loaded).toEqual([1, 1, 1, 1]);
  await expect(page.locator("body")).toHaveCSS("font-family", /Libron/);
  await expect(page.locator("[data-font-preview=transcript]")).toHaveCSS(
    "font-family",
    /Libron/,
  );
  expect(failures.filter((url) => url.includes("/fonts/"))).toEqual([]);
});

test("font roles persist independently, accept installed names, and reset", async ({
  page,
}) => {
  await openFonts(page);
  await selectFont(page, "Interface font", "Verdana");
  await expect(page.locator("body")).toHaveCSS("font-family", /^Verdana/);
  await selectFont(page, "Menu font", "System monospace");
  await expect(page.getByRole("navigation")).toHaveCSS(
    "font-family",
    /^ui-monospace/,
  );
  await selectFont(page, "Overlay font", "Georgia");
  await selectFont(page, "Transcript font", "Arial");
  // A family absent from the preset menu still survives the creatable path.
  const numbers = page.getByRole("combobox", {
    name: "Code and numbers font",
    exact: true,
  });
  await numbers.fill("Missing Font For Fallback");
  await page
    .getByRole("option", {
      name: "Use installed font “Missing Font For Fallback”",
      exact: true,
    })
    .click();
  await expect(numbers).toBeEnabled();
  await page.reload();
  await page
    .getByRole("navigation")
    .getByRole("button", { name: "About", exact: true })
    .click();
  for (const [role, family] of [
    ["ui", "Verdana"],
    ["overlay", "Georgia"],
    ["transcript", "Arial"],
    ["mono", "Missing Font For Fallback"],
  ]) {
    await expect(page.locator(`[data-font-preview=${role}]`)).toHaveCSS(
      "font-family",
      new RegExp(family),
    );
  }
  await expect(page.getByRole("navigation")).toHaveCSS(
    "font-family",
    /^ui-monospace/,
  );
  await expect(page.locator("[data-font-preview=mono]")).toHaveCSS(
    "font-family",
    /Libron/,
  );
  await page
    .getByRole("button", { name: "Reset fonts to Libron", exact: true })
    .click();
  await expect(page.locator("body")).toHaveCSS("font-family", /^Libron/);
  await expect(page.getByRole("navigation")).toHaveCSS(
    "font-family",
    /^Libron/,
  );
  await expect(page.locator("[data-font-preview=transcript]")).toHaveCSS(
    "font-family",
    /^Libron/,
  );
  await page.screenshot({
    path: "test-results/fonts-desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 1060, height: 720 });
  await page
    .getByRole("heading", { name: "Fonts", exact: true })
    .scrollIntoViewIfNeeded();
  await page.screenshot({ path: "test-results/fonts-minimum.png" });
  expect(
    await page.evaluate(() => (window as any).__TAURI_UNHANDLED__),
  ).toEqual([]);
});

test("overlay font and transcript font update independently through the live event", async ({
  page,
}) => {
  const crashes: string[] = [];
  page.on("pageerror", (error) => crashes.push(error.message));
  await page.goto("/src/overlay/index.html");
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).__TAURI_CALLS__.some(
          (c: any) =>
            c.cmd === "plugin:event|listen" && c.args.event === "show-overlay",
        ),
      ),
    )
    .toBe(true);
  await page.evaluate(async () => {
    const invoke = (window as any).__TAURI_INTERNALS__.invoke;
    await invoke("plugin:event|emit", {
      event: "show-overlay",
      payload: "streaming",
    });
    await invoke("plugin:event|emit", {
      event: "stream-text-event",
      payload: {
        committed: "A legible transcript. Écriture, 日本語, العربية.",
        tentative: "",
        slot: null,
      },
    });
    await invoke("plugin:event|emit", {
      event: "typography-changed",
      payload: {
        ui: "Verdana",
        menus: "inherit",
        overlay: "Georgia",
        transcript: "Arial",
        mono: "inherit",
      },
    });
  });
  await expect(page.locator(".scard")).toHaveCSS("font-family", /^Georgia/);
  await expect(page.locator(".stext-col p").first()).toHaveCSS(
    "font-family",
    /^Arial/,
  );
  await expect(page.locator(".stext-col p").first()).toContainText(
    "A legible transcript.",
  );
  await page.screenshot({
    path: "test-results/fonts-overlay.png",
    omitBackground: true,
  });
  expect(crashes).toEqual([]);
});

test("live transcript follows its font while UI keeps its own font", async ({
  page,
}) => {
  await openFonts(page);
  await selectFont(page, "Interface font", "Verdana");
  await selectFont(page, "Transcript font", "Georgia");
  await page
    .getByRole("navigation")
    .getByRole("button", { name: "Live Mode", exact: true })
    .click();
  await expect(page.locator(".transcript-text")).toHaveCSS(
    "font-family",
    /^Georgia/,
  );
  await expect(page.locator("body")).toHaveCSS("font-family", /^Verdana/);
});
