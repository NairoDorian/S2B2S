// Phase 0(b): the migration safety net for the settings window.
//
// Why this file exists. `tests/app.spec.ts` asserts the dev server answers 200
// and that `<html>`/`<body>` exist. A React 19 → Solid 2 rewrite of 199 files
// passes both of those with a completely blank page, so those two tests are not
// a net — they are a heartbeat. This file is the net.
//
// Two rules make it survive the swap:
//
//   1. **Assert on roles, headings and the app's own IPC — never on framework
//      internals.** Not one assertion here mentions React, Solid, a hook, a
//      component name or a prop. What it describes is the window's behaviour,
//      which is the same contract on both sides.
//   2. **Pick anchors the app owns for its own reasons.** The sidebar entries
//      are `<button>`s inside a `<nav>` with `aria-current`; the pages are
//      `h1`–`h3`; the overlay's cancel control is a button labelled "cancel".
//      None of that exists for the tests. There are deliberately no
//      `data-testid`s: `src/` has none, and the app should not grow test-only
//      attributes for the migration's benefit — an attribute nothing else uses
//      is exactly the thing that rots silently.
//
// The mocks live in `harness/`: `fixtures.ts` boots the app with the two init
// scripts it needs, `tauri-mock.js` answers the IPC, and `fixtures.generated.ts`
// is generated from `src/bindings.ts` so payload shapes cannot drift. See the
// headers there.
import { test, expect } from "./harness/fixtures";
import type { Page } from "@playwright/test";

/**
 * Every sidebar page, with a heading that only that page renders.
 *
 * Verified against the running app, not assumed: `getByRole("heading")` cannot
 * collide with the sidebar, because the sidebar renders buttons. That is what
 * lets "History" mean the History *page* without disambiguating against the
 * "History" nav entry.
 */
const SECTIONS: { label: string; heading: string }[] = [
  { label: "General", heading: "General" },
  { label: "History", heading: "History" },
  { label: "Statistics", heading: "Transcription Statistics" },
  { label: "Models", heading: "Transcription Models" },
  { label: "Multi STT", heading: "Enable Multi STT" },
  { label: "Transcribe Files", heading: "1. Audio files" },
  { label: "Live Mode", heading: "Live Mode" },
  { label: "Live FFT", heading: "Live FFT" },
  { label: "Overlay", heading: "Appearance" },
  { label: "Local LLM", heading: "Server" },
  { label: "Advanced", heading: "Start Hidden" },
  { label: "Debug", heading: "Debug" },
  { label: "Help", heading: "Help" },
  { label: "About", heading: "About" },
];

/** Navigate and wait for the shell. Before this resolves the app has mounted. */
async function boot(page: Page) {
  await page.goto("/");
  await expect(sidebarButton(page, "General")).toHaveAttribute(
    "aria-current",
    "page",
  );
}

/** A sidebar entry. Scoped to `<nav>` so a same-named button in a page cannot match. */
function sidebarButton(page: Page, label: string) {
  return page
    .getByRole("navigation")
    .getByRole("button", { name: label, exact: true });
}

/** Commands the mock was asked for but had no answer to. See `tauri-mock.js`. */
function unhandled(page: Page) {
  return page.evaluate(() => [
    ...new Set((window as any).__TAURI_UNHANDLED__ as string[]),
  ]);
}

/** Every `invoke` the app has made, in order. */
function calls(page: Page) {
  return page.evaluate(
    () => (window as any).__TAURI_CALLS__ as { cmd: string; args: any }[],
  );
}

/** The model writes the combobox has sent to the backend, in order. */
async function modelWrites(page: Page) {
  return (await calls(page)).filter(
    (c) => c.cmd === "change_post_process_model_setting",
  );
}

/**
 * How many `listen` registrations for `event` the app has made, ever.
 *
 * Cumulative on purpose: a registration can be superseded (the effect's deps
 * changed, so it tore the old one down and bound a new one), and the count only
 * ever grows. What it is used for is *growth* — see the language-switch test.
 */
async function listenerRegistrations(page: Page, event: string) {
  return (await calls(page)).filter(
    (c) => c.cmd === "plugin:event|listen" && c.args.event === event,
  ).length;
}

/**
 * Click a settings toggle.
 *
 * The input is `sr-only` — 1×1 px, clipped — so it is deliberately not the
 * thing a user hits, and Playwright refuses to click it for exactly that
 * reason. The visible switch is the styled sibling inside the same `<label>`,
 * so clicking the label is what a mouse actually does; the browser then
 * activates the input and fires the change event.
 */
async function clickToggle(page: Page, name: string) {
  const input = page.getByRole("checkbox", { name });
  await input.locator("xpath=ancestor::label[1]").click();
}

/**
 * Wait until the page has registered a listener for `event`.
 *
 * `plugin:event|emit` in the mock delivers to whoever is listening *at that
 * instant* and drops the event otherwise — there is no queue and no replay. The
 * app registers its listeners from an async effect (`await listen(...)`), so an
 * emit issued as soon as a navigation settles can land before that await
 * resolves; the test then fails for a reason that has nothing to do with the
 * app. This was not theoretical: the overlay test passed in isolation and
 * failed under a full-suite run.
 *
 * The registration is observable because the mock records every `invoke`, so
 * this waits on the fact rather than on a guessed delay.
 */
async function waitForListener(page: Page, event: string) {
  await expect
    .poll(async () =>
      (await calls(page)).some(
        (c) => c.cmd === "plugin:event|listen" && c.args.event === event,
      ),
    )
    .toBe(true);
}

/**
 * Any uncaught error unmounts the entire window — the sidebar included, because
 * the pages sit outside the one `ErrorBoundary` in the tree. So this is not
 * "log the noise": a page that reads a field off a null payload takes the whole
 * app down, and the failure surfaces as `#root` being empty. Collected here so
 * every test fails with the real message rather than a confusing empty body.
 */
function watchForCrashes(page: Page): string[] {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  return errors;
}

test.describe("settings window", () => {
  test("boots into the General page", async ({ page }) => {
    const crashes = watchForCrashes(page);

    await boot(page);

    await expect(page.getByRole("heading", { name: "General" })).toBeVisible();
    // The shell's furniture, from the footer. The version string is served by
    // the mock's `plugin:app|version`, so this also proves the app's own
    // startup IPC reached the UI — a page rendering inside a dead frame would
    // otherwise still pass.
    await expect(page.getByText("v0.0.0-test")).toBeVisible();
    expect(crashes).toEqual([]);
  });

  test("every sidebar section renders its page", async ({ page }) => {
    const crashes = watchForCrashes(page);

    await boot(page);

    for (const { label, heading } of SECTIONS) {
      await sidebarButton(page, label).click();

      // The heading proves the right page mounted; `aria-current` proves the
      // sidebar tracked it. Both, because a page rendering while the sidebar
      // desynchronises is a real failure mode (and is what the Phase 3 store
      // migration would break first).
      //
      // `.first()`: a page may legitimately carry its title twice — once as the
      // group's `<h2>` and once as the row's `<h3>` ("Multi STT" does) — so an
      // exact match can resolve to two elements. Either proves it mounted.
      await expect(
        page.getByRole("heading", { name: heading, exact: true }).first(),
      ).toBeVisible({ timeout: 15_000 });
      await expect(sidebarButton(page, label)).toHaveAttribute(
        "aria-current",
        "page",
      );

      // A crash unmounts everything, so checking here names the page that did it
      // instead of failing later at an unrelated assertion.
      expect(crashes, `navigating to "${label}" crashed the window`).toEqual(
        [],
      );
    }

    // Walked *after* the sweep: every command these pages reach for at mount or
    // on an interval must be one the harness answers. Asserting this is what
    // makes the list in `tauri-mock.js` a closed set — a new call site fails
    // here, loudly, instead of rendering a blank corner nobody looks at.
    expect(await unhandled(page)).toEqual([]);
  });

  test("a settings toggle reaches the backend", async ({ page }) => {
    const crashes = watchForCrashes(page);

    await boot(page);
    await sidebarButton(page, "Advanced").click();

    // `Starting Hidden` is a real persisted setting with a real command behind
    // it (`change_start_hidden_setting`), so this exercises the whole path:
    // control → store → updater → IPC.
    const toggle = page.getByRole("checkbox", { name: "Start Hidden" });
    await expect(toggle).not.toBeChecked();

    const before = (await calls(page)).filter(
      (c) => c.cmd === "change_start_hidden_setting",
    );
    expect(before).toEqual([]);

    await clickToggle(page, "Start Hidden");

    // The store side: the optimistic local write has already landed.
    await expect(toggle).toBeChecked();
    // The persistence side: the value that actually left the webview.
    await expect
      .poll(async () =>
        (await calls(page)).filter(
          (c) => c.cmd === "change_start_hidden_setting",
        ),
      )
      .toEqual([
        { cmd: "change_start_hidden_setting", args: { enabled: true } },
      ]);

    expect(await unhandled(page)).toEqual([]);
    expect(crashes).toEqual([]);
  });

  test("a backend event surfaces as a toast", async ({ page }) => {
    const crashes = watchForCrashes(page);

    await boot(page);
    await waitForListener(page, "recording-error");

    // The app listens for this at the shell and turns it into a toast. Emitting
    // it through the mock is the only way to reach that listener without a
    // microphone, and it exercises the event plugin end to end.
    await page.evaluate(() =>
      (window as any).__TAURI_INTERNALS__.invoke("plugin:event|emit", {
        event: "recording-error",
        payload: { error_type: "no_input_device", detail: null },
      }),
    );

    // Text from the English locale, which the fixture pins via `app_language`.
    //
    // `.first()` because the dev build mounts under `React.StrictMode`, whose
    // simulated unmount/remount does register this listener twice: measured,
    // one emit here produces 2 `plugin:event|listen` and 1
    // `plugin:event|unlisten` — the first binding is torn down and the second
    // one answers. StrictMode is dev-only, has no Solid counterpart and is
    // deleted in Phase 3, so the extra registration is an artefact of the dev
    // entry point rather than of the app. What is under test is that the event
    // reaches the UI at all.
    await expect(page.getByText("No Microphone Found").first()).toBeVisible();
    expect(crashes).toEqual([]);
  });
});

/**
 * The app's one combobox (`components/ui/Select.tsx`).
 *
 * Phase 1 step 4 replaced `react-select` with a rewrite, and this is the most
 * behaviour-dense control the migration touches: a filter, a creatable
 * pseudo-option, its own keyboard map, and three separate rules about what
 * closing does to the value. Every one of those was read out of react-select's
 * `dist/` rather than guessed, so each deserves an assertion that would fail if
 * the rewrite got it wrong.
 *
 * It is reachable only through the Post Process page, which the generated
 * fixture leaves switched off — hence the override, which exists so the page
 * *exists* and so the filter has one option to work on. Nothing below asserts on
 * a value that came from the fixture.
 */
test.describe("post-processing model combobox", () => {
  test.use({
    settingsOverrides: {
      post_process_enabled: true,
      post_process_models: { openai: "gpt-4o" },
    },
  });

  /**
   * The control's own box — the input and its two indicators, *without* the
   * popup, which is a sibling of it. `xpath=../..` from the input: input →
   * wrapper → control. Scoping to it is what lets `toContainText` mean "the
   * value the control is showing" rather than "somewhere on the page".
   */
  function control(page: Page) {
    return page.getByRole("combobox", { name: "Model" }).locator("xpath=../..");
  }

  /** Reach the page. The sidebar entry only exists with the override above. */
  async function openPage(page: Page) {
    await boot(page);
    await sidebarButton(page, "Post Process").click();
    const field = page.getByRole("combobox", { name: "Model" });
    await expect(field).toBeVisible();
    return field;
  }

  test("filters, and creates a model that is not in the list", async ({
    page,
  }) => {
    const crashes = watchForCrashes(page);

    const field = await openPage(page);

    // The committed value, shown by the control while nothing is typed. The
    // rewrite draws this itself rather than react-select's hidden value node, so
    // it is worth pinning: without it the control would look empty.
    await expect(control(page)).toContainText("gpt-4o");

    await field.click();
    const options = page.getByRole("option");
    await expect(options).toHaveText(["gpt-4o"]);

    // A query nothing matches empties the list — and the create row takes its
    // place, which is only visible because the real option is gone. One
    // assertion, both halves of the filter.
    await field.fill("zz");
    await expect(options).toHaveText(['Use "zz"']);

    await options.click();

    // A create commits through the backend and nothing else: the parent owns the
    // new value, so there is no `onChange` alongside it. That is react-select's
    // behaviour too (`useCreatable` returns before calling the wrapped handler),
    // and it is the half a browser cannot observe without the IPC record.
    await expect
      .poll(async () => modelWrites(page))
      .toEqual([
        {
          cmd: "change_post_process_model_setting",
          args: { providerId: "openai", model: "zz" },
        },
      ]);

    expect(await unhandled(page)).toEqual([]);
    expect(crashes).toEqual([]);
  });

  test("clears on Backspace, and Escape only closes", async ({ page }) => {
    const crashes = watchForCrashes(page);

    const field = await openPage(page);

    // `backspaceRemovesValue` — with nothing typed, Backspace is the keyboard's
    // clear, and the clear is an empty model rather than no write at all. This
    // is the path the clear indicator exists for, and unlike the indicator it
    // needs no positional selector to reach.
    await field.press("Backspace");
    await expect
      .poll(async () => modelWrites(page))
      .toEqual([
        {
          cmd: "change_post_process_model_setting",
          args: { providerId: "openai", model: "" },
        },
      ]);

    // `escapeClearsValue` is *false*, so Escape closes and does nothing else.
    // The write above has already landed, which is what makes the count below
    // mean something: it can only still be one if Escape wrote nothing.
    await field.press("ArrowDown");
    const options = page.getByRole("option");
    await expect(options).toHaveText(["gpt-4o"]);

    await field.press("Escape");
    await expect(options).toBeHidden();
    await expect(control(page)).toContainText("gpt-4o");

    expect(await modelWrites(page)).toHaveLength(1);
    expect(await unhandled(page)).toEqual([]);
    expect(crashes).toEqual([]);
  });
});

/**
 * The language switch (Phase 1 step 5).
 *
 * This is the assertion for the in-house `useTranslation()`'s one non-obvious
 * duty. Handing out `t` was never the hard part — `i18next` core does the
 * translating and was already a dependency. What `react-i18next` supplied was
 * React's half: making all 116 components that translate text re-render when the
 * language changes, which is why its `t` is a *different function* after the
 * switch. Measured across `src/`: **18 dependency arrays list `t`** — 10
 * `useEffect`, 4 `useCallback`, 4 `useMemo` — and what each of those deps buys
 * is that the hook re-runs under the new language instead of keeping the result
 * it computed under the old one.
 *
 * Of those 18, exactly one kind of re-run is observable from a browser, and that
 * is what the `listenerRegistrations` assertion pins: the shell's five backend
 * listeners in `App.tsx` translate inside their callbacks, so a language change
 * has to tear each one down and bind a new one — visible in the recorded IPC.
 * The four `useMemo`s hold something derived from translated strings
 * (`ModelsSettings`'s selected-language label and `LiveFftSettings`'s canvas
 * labels cache the text itself, `HelpSettings`'s the search result whose
 * matching runs over it, `HistorySettings`'s the speed rating) and every one of
 * them sits on a page that cannot be on screen at the same time as the About
 * page's selector — so a switch unmounts the memo and it recomputes on the way
 * back in regardless. That is why the listener count carries the weight here
 * rather than one of the four.
 *
 * The toast below is deliberately the *weaker* half, and worth being exact
 * about why: `i18n.t` reads the current language when it is *called*, so even a
 * listener holding a stale `t` translates into the new language. The toast
 * proves the re-bound listener is live and that the German bundle is what it
 * translates with; the count is what fails when the hook stops handing out a new
 * `t`.
 *
 * The switch is made through the app's own control on the About page, and the
 * first assertion lands on a component the switch did *not* happen in — the
 * sidebar, rendered by `Sidebar.tsx` from a different store — so what it proves
 * is that the window re-rendered, not that one control did.
 *
 * Two things around it are Phase 1's business as well: the German bundle has to
 * arrive as its own chunk over the wire (the non-eager locale glob in
 * `src/i18n/index.ts` is still doing its job), and the switch has to reach the
 * backend as a settings write.
 */
test.describe("language switch", () => {
  test("re-renders the window in the chosen language", async ({ page }) => {
    const crashes = watchForCrashes(page);

    await boot(page);

    // The shell binds this at mount and translates inside its callback, so it
    // is the one place the hook's per-language `t` is observable from here.
    await waitForListener(page, "recording-error");
    const bindingsWhileEnglish = await listenerRegistrations(
      page,
      "recording-error",
    );

    await sidebarButton(page, "About").click();

    // The selector's button carries the current choice, and its options carry
    // each language's own name for itself.
    await page.getByRole("button", { name: "English (English)" }).click();
    await page.getByRole("button", { name: "Deutsch (German)" }).click();

    // "Info" is `sidebar.about` in German, and it is still the current page —
    // the same entry, renamed, one component away from where the click landed.
    await expect(sidebarButton(page, "Info")).toHaveAttribute(
      "aria-current",
      "page",
    );
    await expect(sidebarButton(page, "About")).toHaveCount(0);

    // The switch reached the backend as a settings write.
    await expect
      .poll(async () =>
        (await calls(page)).filter(
          (c) => c.cmd === "change_app_language_setting",
        ),
      )
      .toEqual([
        { cmd: "change_app_language_setting", args: { language: "de" } },
      ]);

    // The load-bearing assertion: the listeners were re-bound. A hook that
    // handed out one `t` for the life of the window would leave this count
    // exactly where it started.
    await expect
      .poll(async () => listenerRegistrations(page, "recording-error"))
      .toBeGreaterThan(bindingsWhileEnglish);

    // And the re-bound listener works, in German.
    await page.evaluate(() =>
      (window as any).__TAURI_INTERNALS__.invoke("plugin:event|emit", {
        event: "recording-error",
        payload: { error_type: "no_input_device", detail: null },
      }),
    );
    await expect(
      page.getByText("Kein Mikrofon gefunden").first(),
    ).toBeVisible();

    expect(await unhandled(page)).toEqual([]);
    expect(crashes).toEqual([]);
  });
});

test.describe("recording overlay window", () => {
  test("mounts, and renders only once the backend shows it", async ({
    page,
  }) => {
    const crashes = watchForCrashes(page);

    await page.goto("/src/overlay/index.html");

    // The overlay is its own webview with its own entry point. It starts hidden
    // and stays unmounted — so an empty `#root` here is the correct state, and
    // asserting it is what makes the next assertion meaningful: it proves the
    // card below was rendered *in response to the event*, not always present.
    //
    // Ordered deliberately: wait for the listener FIRST, so the empty `#root`
    // means "mounted and listening, nothing rendered yet" rather than "the
    // bundle has not executed". Without that the assertion passes either way
    // and proves nothing.
    await waitForListener(page, "show-overlay");
    await expect(page.locator("#root")).toBeEmpty();

    await page.evaluate(() =>
      (window as any).__TAURI_INTERNALS__.invoke("plugin:event|emit", {
        event: "show-overlay",
        payload: "recording",
      }),
    );

    // The cancel control is the card's one labelled control, present in every
    // recording state.
    await expect(page.getByRole("button", { name: "cancel" })).toBeVisible();
    expect(crashes).toEqual([]);
  });
});
