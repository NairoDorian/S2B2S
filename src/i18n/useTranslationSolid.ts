import { createSignal } from "solid-js";
import type { TFunction } from "i18next";
import i18n from "./index";

/**
 * The Solid counterpart to `./useTranslation.tsx` — the same two names at the
 * same call sites, but built on Solid's reactivity instead of React's.
 *
 * Phase 1 of docs/PLAN_SOLIDJS_2.md replaced `react-i18next` with the React hook
 * next door; Phase 2 makes `src/overlay/` Solid and that hook cannot cross the
 * boundary. This one is deliberately tiny, because the two runtimes need
 * opposite things from it:
 *
 * - **React** has to be *told* a translation changed. `useTranslation.tsx`
 *   therefore subscribes the component with `useSyncExternalStore` and hands out
 *   a memoized `t`, and every one of its 116 call sites re-renders on a language
 *   change.
 * - **Solid** has to be told nothing. A component body runs once, and a
 *   `t("…")` written in JSX is compiled into its own computation — so all this
 *   hook has to do is *read something reactive while that computation runs*,
 *   and the text node updates itself when the language changes.
 *
 * That read is the `language()` call inside `t` below, and it is the whole
 * mechanism: `i18n.t` is a live function that resolves against `i18n.language`
 * when it is *called*, so its return value is right even without the read. The
 * read exists for the subscription, not the string.
 *
 * Reading it inside `t` rather than at the top of the component body is also
 * what keeps it legal. A component body is an owned scope, and a reactive read
 * there is both a one-time snapshot and a development warning (`STRICT_READ_
 * UNTRACKED`); inside the JSX computation the read is tracked, scoped to the one
 * binding that needs it, and correct. Outside any computation — in an event
 * handler, say — the read is inert, which is also what is wanted: a handler
 * wants the current language, not a subscription to it.
 */
const [language, setLanguage] = createSignal(i18n.language);

// A module-scope signal rather than a context: this is genuinely app-wide state
// with no subtree to scope it to, and `src/i18n/index.ts` is already a module
// singleton both windows import. (In Solid a module-scope signal *is* a global —
// that is the sanctioned shape, not a workaround.)
i18n.on("languageChanged", (lng) => setLanguage(lng));

/**
 * The current UI language, as a reactive accessor.
 *
 * Exported for the one case `t` cannot serve: a value *derived* from the
 * language rather than translated by it. The overlay's writing direction is the
 * example — `getLanguageDirection(i18n.language)` has to re-run on a language
 * change, and reading a bare `i18n.language` would freeze it at whatever it was
 * when the component body ran.
 */
export const currentLanguage = language;

export function useTranslation(): { t: TFunction; i18n: typeof i18n } {
  const t = ((key: string, options?: Record<string, unknown>) => {
    language();
    return i18n.t(key, options);
  }) as unknown as TFunction;

  return { t, i18n };
}
