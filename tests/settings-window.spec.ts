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
  { label: "Recall", heading: "Recall" },
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

  test("the spoken-languages picker adds and removes a language", async ({
    page,
  }) => {
    const crashes = watchForCrashes(page);
    await boot(page);

    const writes = async () =>
      (await calls(page)).filter(
        (c) => c.cmd === "change_spoken_languages_setting",
      );
    // Off by default, and the letters toggle only shows once a language is in.
    await expect(
      page.getByRole("checkbox", { name: "Allow foreign words" }),
    ).toHaveCount(0);

    await page.getByRole("button", { name: "Add a language" }).click();
    const search = page.getByRole("textbox", { name: "Search languages" });
    await expect(search).toBeFocused();
    await search.fill("fren");
    await search.press("Enter");

    await expect
      .poll(writes)
      .toEqual([
        { cmd: "change_spoken_languages_setting", args: { languages: ["fr"] } },
      ]);
    const remove = page.getByRole("button", { name: "Remove French" });
    await expect(remove).toBeVisible();
    await expect(
      page.getByRole("checkbox", { name: "Allow foreign words" }),
    ).toBeVisible();

    // Escape clears the query first, then closes and hands focus back.
    await search.fill("ger");
    await search.press("Escape");
    await expect(search).toHaveValue("");
    await search.press("Escape");
    await expect(search).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: "Add another" }),
    ).toBeFocused();

    await remove.click();
    await expect
      .poll(async () => (await writes()).at(-1))
      .toEqual({
        cmd: "change_spoken_languages_setting",
        args: { languages: [] },
      });

    expect(await unhandled(page)).toEqual([]);
    expect(crashes).toEqual([]);
  });

  test("an empty list offers the system's languages, and applies them only on a click", async ({
    page,
  }) => {
    const crashes = watchForCrashes(page);
    await boot(page);
    // The harness's OS languages are fr-FR then en-US.
    await expect(
      page.getByText("From your system: French, English"),
    ).toBeVisible();
    // Shown, not applied: nothing is written until the user asks.
    expect(
      (await calls(page)).filter(
        (c) => c.cmd === "change_spoken_languages_setting",
      ),
    ).toEqual([]);

    await page.getByRole("button", { name: "Add them" }).click();
    await expect
      .poll(async () =>
        (await calls(page)).filter(
          (c) => c.cmd === "change_spoken_languages_setting",
        ),
      )
      .toEqual([
        {
          cmd: "change_spoken_languages_setting",
          args: { languages: ["fr", "en"] },
        },
      ]);
    await expect(
      page.getByRole("button", { name: "Remove French" }),
    ).toBeVisible();
    await expect(page.getByText(/^From your system:/)).toHaveCount(0);

    expect(await unhandled(page)).toEqual([]);
    expect(crashes).toEqual([]);
  });

  test("speaking Chinese reveals the script setting", async ({ page }) => {
    const crashes = watchForCrashes(page);
    await boot(page);

    const scriptRow = page.getByText("Chinese Script", { exact: true });
    await expect(scriptRow).toHaveCount(0);

    await page.getByRole("button", { name: "Add a language" }).click();
    const search = page.getByRole("textbox", { name: "Search languages" });
    await search.fill("cantonese");
    await search.press("Enter");
    await search.press("Escape");
    await expect(scriptRow).toBeVisible();

    await page.getByRole("button", { name: "As Transcribed" }).click();
    await page
      .getByRole("button", { name: /Traditional \(Hong Kong\)/ })
      .click();
    await expect
      .poll(async () =>
        (await calls(page)).filter(
          (c) => c.cmd === "change_chinese_script_setting",
        ),
      )
      .toEqual([
        {
          cmd: "change_chinese_script_setting",
          args: { script: "traditional_hk" },
        },
      ]);

    expect(await unhandled(page)).toEqual([]);
    expect(crashes).toEqual([]);
  });

  test("a missing language detector is explained and can be downloaded", async ({
    page,
  }) => {
    const crashes = watchForCrashes(page);
    // The selected model cannot detect French vs English on its own and the
    // detector is not on disk yet.
    await page.addInitScript(() => {
      (window as any).__TAURI_MOCK_REPLIES__ = {
        get_language_status: { kind: "detector_missing", fallback: "fr" },
      };
    });
    await boot(page);

    // The line only shows with a list: add two languages.
    await page.getByRole("button", { name: "Add a language" }).click();
    const search = page.getByRole("textbox", { name: "Search languages" });
    for (const name of ["french", "english"]) {
      await search.fill(name);
      await search.press("Enter");
    }
    await search.press("Escape");

    await expect(
      page.getByText(/until it is downloaded, Auto transcribes in French/),
    ).toBeVisible();
    await page.getByRole("button", { name: "Download the detector" }).click();
    await expect
      .poll(async () =>
        (await calls(page)).some((c) => c.cmd === "download_language_detector"),
      )
      .toBe(true);

    expect(await unhandled(page)).toEqual([]);
    expect(crashes).toEqual([]);
  });

  test("a History row shows the language it came out in", async ({ page }) => {
    const crashes = watchForCrashes(page);
    const row = (id: number, language: string, source: string | null) => ({
      id,
      file_name: `recording-${id}.wav`,
      timestamp: 1_700_000_000 + id,
      saved: false,
      title: `Recording ${id}`,
      transcription_text: `transcript ${id}`,
      post_processed_text: null,
      post_process_prompt: null,
      post_process_requested: false,
      model_id: "parakeet",
      engine: null,
      audio_duration_ms: 2000,
      speech_duration_ms: 1500,
      sample_rate_hz: 48000,
      word_count: 2,
      transcription_latency_ms: 120,
      post_processing_latency_ms: null,
      language,
      language_source: source,
      mode: "single",
      extra_models: null,
    });
    // One row the detector chose French for, and one saved before the source
    // was recorded, whose column still holds the language *setting*.
    await page.addInitScript(
      (entries) => {
        (window as any).__TAURI_MOCK_REPLIES__ = {
          get_history_entries: { entries, has_more: false },
        };
      },
      [row(2, "fr", "detected"), row(1, "auto", null)],
    );
    await boot(page);
    await sidebarButton(page, "History").click();

    const chips = page.locator('[title^="Transcribed in"]');
    await expect(chips).toHaveCount(1);
    await expect(chips).toHaveText("French");
    await expect(chips).toHaveAttribute("title", /language detector/);

    expect(await unhandled(page)).toEqual([]);
    expect(crashes).toEqual([]);
  });

  test("a backend event surfaces as a toast", async ({ page }) => {
    const crashes = watchForCrashes(page);

    await boot(page);
    await waitForListener(page, "notices-changed");

    // A recording that could not start is a notice; the notice store pops a
    // newly raised one up as a toast. Emitting it through the mock is the only
    // way to reach that listener without a microphone, and it exercises the
    // event plugin end to end.
    await page.evaluate(() =>
      (window as any).__TAURI_INTERNALS__.invoke("plugin:event|emit", {
        event: "notices-changed",
        payload: {
          notices: [
            {
              code: "NoMicrophone",
              level: "error",
              source: "dictation",
              detail: null,
            },
          ],
          raised: {
            code: "NoMicrophone",
            level: "error",
            source: "dictation",
            detail: null,
          },
        },
      }),
    );

    // Text from the English locale, which the fixture pins via `app_language`.
    //
    // `.first()` as defence in depth: the tree mounts exactly once under
    // Solid (no StrictMode — that was React's dev-only double render, gone
    // since Phase 3), so a single emit produces one listener. Keeping
    // `.first()` costs nothing and keeps the assertion about the event
    // reaching the UI rather than about registration counts.
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
 * listeners in `App.tsx` translate inside their callbacks with a `t` that
 * resolves the language when it is *called*, so they are bound once for the
 * life of the window and a language change must NOT tear them down and bind
 * them again (it used to, five IPC round-trips per switch for nothing, with a
 * window in which two listeners were live at once) — visible in the recorded
 * IPC as a count that does not move.
 * The four `useMemo`s hold something derived from translated strings
 * (`ModelsSettings`'s selected-language label and `LiveFftSettings`'s canvas
 * labels cache the text itself, `HelpSettings`'s the search result whose
 * matching runs over it, `HistorySettings`'s the speed rating) and every one of
 * them sits on a page that cannot be on screen at the same time as the About
 * page's selector — so a switch unmounts the memo and it recomputes on the way
 * back in regardless. That is why the listener count carries the weight here
 * rather than one of the four.
 *
 * The toast below is the other half: the listener bound in English is still
 * live after the switch and translates with the German bundle, which is what
 * makes binding it once correct.
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
/**
 * A Multi-STT slot explains its own language plan. Slot "Model 2" runs a model
 * that cannot tell French from English by itself, the user speaks both, and the
 * detector is not on disk: the slot says what Auto falls back to and offers the
 * download, exactly as the General page's line does for the primary model.
 */
test.describe("Multi-STT slot language", () => {
  test.use({
    settingsOverrides: {
      multi_stt_enabled: true,
      spoken_languages: ["fr", "en"],
      multi_stt_extra_models: [{ model_id: "canary-test", language: null }],
    },
  });

  test("a slot explains its plan and offers the detector", async ({ page }) => {
    const crashes = watchForCrashes(page);
    await page.addInitScript(() => {
      (window as any).__TAURI_MOCK_REPLIES__ = {
        get_available_models: [
          {
            id: "canary-test",
            name: "Canary Test",
            description: "",
            filename: "canary-test.gguf",
            source: {
              Url: { url: "https://example.invalid/canary-test.gguf" },
            },
            size_mb: 1,
            is_downloaded: true,
            is_downloading: false,
            partial_size: null,
            accuracy_score: null,
            speed_score: null,
            supports_translation: false,
            is_recommended: false,
            supported_languages: ["en", "fr", "de"],
            supports_language_selection: true,
            is_custom: false,
            supports_streaming: false,
            supports_language_detection: false,
          },
        ],
        get_multi_stt_language_statuses: [
          { kind: "detector_missing", fallback: "fr" },
        ],
      };
    });
    await boot(page);
    await sidebarButton(page, "Multi STT").click();

    await expect(
      page.getByText(/until it is downloaded, Auto transcribes in French/),
    ).toBeVisible();
    await page.getByRole("button", { name: "Download the detector" }).click();
    await expect
      .poll(async () =>
        (await calls(page)).some((c) => c.cmd === "download_language_detector"),
      )
      .toBe(true);
    // The list is asked for again after the download.
    await expect
      .poll(
        async () =>
          (await calls(page)).filter(
            (c) => c.cmd === "get_multi_stt_language_statuses",
          ).length,
      )
      .toBeGreaterThan(1);

    expect(await unhandled(page)).toEqual([]);
    expect(crashes).toEqual([]);
  });
});

/**
 * The custom post-processing provider can speak the Anthropic Messages API
 * (upstream 14ae9c6): the format is offered for it alone, and a pick reaches
 * the backend command.
 */
test.describe("post-processing API format", () => {
  const provider = (id: string, label: string) => ({
    id,
    label,
    base_url:
      id === "custom"
        ? "http://localhost:11434/v1"
        : "https://api.openai.com/v1",
    allow_base_url_edit: id === "custom",
    models_endpoint: "/models",
    supports_structured_output: id !== "custom",
    api_format: "openai",
  });
  test.use({
    settingsOverrides: {
      post_process_enabled: true,
      post_process_provider_id: "custom",
      post_process_providers: [
        provider("openai", "OpenAI"),
        provider("custom", "Custom"),
      ],
    },
  });

  test("the custom provider can switch to Anthropic Messages", async ({
    page,
  }) => {
    const crashes = watchForCrashes(page);
    await boot(page);
    await sidebarButton(page, "Post Process").click();

    await expect(page.getByText("API Format", { exact: true })).toBeVisible();
    await page
      .getByRole("button", { name: "OpenAI Compatible (Chat Completions)" })
      .click();
    await page.getByRole("button", { name: "Anthropic Messages" }).click();
    await expect
      .poll(async () =>
        (await calls(page)).filter(
          (c) => c.cmd === "change_post_process_api_format_setting",
        ),
      )
      .toEqual([
        {
          cmd: "change_post_process_api_format_setting",
          args: { providerId: "custom", apiFormat: "anthropic" },
        },
      ]);

    expect(await unhandled(page)).toEqual([]);
    expect(crashes).toEqual([]);
  });
});

test.describe("language switch", () => {
  test("re-renders the window in the chosen language", async ({ page }) => {
    const crashes = watchForCrashes(page);

    await boot(page);

    // The notice store binds this once at mount and translates inside its
    // callback, so it is where a language switch is observable from here.
    await waitForListener(page, "notices-changed");
    const bindingsWhileEnglish = await listenerRegistrations(
      page,
      "notices-changed",
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

    // The listeners are bound once: a language switch does not re-bind them.
    expect(await listenerRegistrations(page, "notices-changed")).toBe(
      bindingsWhileEnglish,
    );

    // And the listener bound in English now speaks German.
    await page.evaluate(() =>
      (window as any).__TAURI_INTERNALS__.invoke("plugin:event|emit", {
        event: "notices-changed",
        payload: {
          notices: [
            {
              code: "NoMicrophone",
              level: "error",
              source: "dictation",
              detail: null,
            },
          ],
          raised: {
            code: "NoMicrophone",
            level: "error",
            source: "dictation",
            detail: null,
          },
        },
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

  test("a dictation notice replaces the card until the next session", async ({
    page,
  }) => {
    const crashes = watchForCrashes(page);
    await page.goto("/src/overlay/index.html");
    await waitForListener(page, "show-overlay");
    await waitForListener(page, "overlay-notice");
    const emit = (event: string, payload: unknown) =>
      page.evaluate(
        ([event, payload]) =>
          (window as any).__TAURI_INTERNALS__.invoke("plugin:event|emit", {
            event,
            payload,
          }),
        [event, payload] as const,
      );

    // What the backend sends: the overlay shown in its session form, then the
    // notice.
    await emit("show-overlay", "transcribing");
    await emit("overlay-notice", {
      code: "MicrophoneSilent",
      level: "warning",
      source: "dictation",
      detail: null,
    });
    const alert = page.getByRole("alert");
    await expect(alert).toContainText("Microphone Is Silent");
    await expect(page.getByRole("button", { name: "cancel" })).toHaveCount(0);

    // A new session takes the overlay back at once.
    await emit("show-overlay", "recording");
    await expect(page.getByRole("alert")).toHaveCount(0);
    await expect(page.getByRole("button", { name: "cancel" })).toBeVisible();
    expect(crashes).toEqual([]);
  });

  test("a slow model load opens the Live panel with a notice", async ({
    page,
  }) => {
    const crashes = watchForCrashes(page);
    await page.goto("/src/overlay/index.html");
    await waitForListener(page, "show-overlay");
    await waitForListener(page, "model-state-changed");

    const emit = (event: string, payload: unknown) =>
      page.evaluate(
        ([event, payload]) =>
          (window as any).__TAURI_INTERNALS__.invoke("plugin:event|emit", {
            event,
            payload,
          }),
        [event, payload] as const,
      );
    const modelEvent = (event_type: string) => ({
      event_type,
      model_id: null,
      model_name: null,
      error: null,
    });

    await emit("show-overlay", "streaming");
    await expect(page.getByRole("button", { name: "cancel" })).toBeVisible();
    await emit("model-state-changed", modelEvent("loading_started"));

    // Nothing for a warm load: the notice waits for the load to be slow.
    const notice = page.getByText("Loading model...");
    await expect(notice).toBeHidden();
    await expect(notice).toBeVisible({ timeout: 5000 });

    // `unloaded` is not ordered against a load in flight and must not end it.
    await emit("model-state-changed", modelEvent("unloaded"));
    await expect(notice).toBeVisible();

    await emit("model-state-changed", modelEvent("loading_completed"));
    await expect(notice).toBeHidden();
    expect(crashes).toEqual([]);
  });
});

test.describe("notices", () => {
  test("a dictation notice with a page of its own opens that page", async ({
    page,
  }) => {
    const crashes = watchForCrashes(page);
    await page.addInitScript(() => {
      (window as any).__TAURI_MOCK_REPLIES__ = {
        get_active_notices: [
          {
            code: "NoModelSelected",
            level: "error",
            source: "dictation",
            detail: null,
          },
        ],
      };
    });
    await boot(page);
    await page.getByRole("button", { name: "1 error" }).click();
    const panel = page.getByRole("dialog", { name: "Notices" });
    await expect(panel.getByText("No Model Selected")).toBeVisible();
    await expect(
      panel.getByText("Download or select a model in Settings > Models."),
    ).toBeVisible();
    await panel.getByRole("button", { name: "Show" }).click();
    await expect(sidebarButton(page, "Models")).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(crashes).toEqual([]);
  });

  test("the footer counts active notices and the panel opens, shows and dismisses them", async ({
    page,
  }) => {
    const crashes = watchForCrashes(page);
    await page.addInitScript(() => {
      (window as any).__TAURI_MOCK_REPLIES__ = {
        get_active_notices: [
          {
            code: "FileTranscriptionFailed",
            level: "error",
            source: "file",
            detail: "meeting.wav: unsupported codec",
          },
          {
            code: "GpuFallback",
            level: "warning",
            source: "gpu",
            detail: "Nemotron 3.5",
          },
        ],
      };
    });
    await boot(page);

    const chip = page.getByRole("button", { name: "1 error, 1 warning" });
    await expect(chip).toBeVisible();
    await chip.click();

    const panel = page.getByRole("dialog", { name: "Notices" });
    await expect(panel.getByText("File Transcription Failed")).toBeVisible();
    await expect(
      panel.getByText("meeting.wav: unsupported codec"),
    ).toBeVisible();
    await expect(
      panel.getByText("Running on CPU After a GPU Error"),
    ).toBeVisible();

    // "Show" opens the source's page and ends that notice.
    await panel.getByRole("button", { name: "Show" }).first().click();
    await expect(sidebarButton(page, "Transcribe Files")).toHaveAttribute(
      "aria-current",
      "page",
    );
    await expect
      .poll(async () =>
        (await calls(page))
          .filter((c) => c.cmd === "dismiss_notice")
          .map((c) => [c.args.source, c.args.code]),
      )
      .toEqual([["file", "FileTranscriptionFailed"]]);
    expect(crashes).toEqual([]);
  });
});

test.describe("transcribe files", () => {
  test("audio dropped on another page opens Transcribe Files with it queued", async ({
    page,
  }) => {
    const crashes = watchForCrashes(page);
    await boot(page);
    await waitForListener(page, "tauri://drag-drop");

    await page.evaluate(() =>
      (window as any).__TAURI_INTERNALS__.invoke("plugin:event|emit", {
        event: "tauri://drag-drop",
        payload: {
          paths: ["C:\audio\meeting.wav", "C:\audio\notes.txt"],
          position: { x: 10, y: 10 },
        },
      }),
    );

    await expect(sidebarButton(page, "Transcribe Files")).toHaveAttribute(
      "aria-current",
      "page",
    );
    await expect(page.getByText("meeting.wav")).toBeVisible();
    await expect(page.getByText("notes.txt")).toHaveCount(0);
    expect(crashes).toEqual([]);
  });

  test("the History option is saved with the page's settings", async ({
    page,
  }) => {
    const crashes = watchForCrashes(page);
    await boot(page);
    await sidebarButton(page, "Transcribe Files").click();
    await clickToggle(page, "Also save to History");
    await expect
      .poll(async () =>
        (await calls(page))
          .filter((c) => c.cmd === "change_file_transcription_settings")
          .map((c) => c.args.settings.save_to_history),
      )
      .toContain(true);
    expect(crashes).toEqual([]);
  });
});
