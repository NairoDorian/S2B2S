// The app's `useTranslation()`.
//
// The React → Solid 2 migration (see CHANGELOG) replaced `react-i18next` with a
// local hook, so the 106 files that imported that package import this path
// instead. Both windows are Solid now, so there is one implementation
// (`./useTranslationSolid`) and this file is a re-export plus the one piece
// `t()` cannot express. Keeping the path means not touching 106 imports.
export { useTranslation, currentLanguage } from "./useTranslationSolid";

/**
 * A translated sentence that carries `<code>` markup, rendered from the tags in
 * the string itself.
 *
 * This is everything the three `<Trans components={{ code: <code /> }} />` call
 * sites were doing. They are the only reason `react-i18next`'s `Trans` was
 * imported, and `Trans` is the one piece of it `t()` cannot express: the
 * sentence is a single translation key that has to come back with part of
 * itself emphasised. Composing it out of two `t()` calls would mean two new
 * keys, and therefore edits to 26 locale files — which are out of scope for
 * this migration. `t()` interpolates and leaves the tags in place, so splitting
 * the result on the tag reproduces the same DOM, in the same translation, with
 * one pass over the locale.
 *
 * Only `<code>` is understood, and only because that is the only tag any locale
 * uses (checked across all 26). A second tag would need this to grow into a real
 * `Trans`, not a general-purpose one that lies about what it handles.
 */
export const TranslatedMarkup = (props: { text: string }) => (
  <>
    {props.text
      .split(/<\/?code>/)
      .map((part, index) => (index % 2 === 1 ? <code>{part}</code> : part))}
  </>
);
