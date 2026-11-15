# Font preferences

The **About → Fonts** section controls five text roles: interface, menus,
overlay controls, transcripts, and code/numbers. Libron is the default interface
font; the other roles follow it until explicitly overridden. Reset restores
that arrangement. A named installed family can be entered in any selector.

`AppSettings.typography` is the persisted source of truth. The serde defaults
handle old stores and partially populated font preferences. The async
`change_typography_setting` command normalizes names, persists on the blocking
pool, and emits one `typography-changed` event. The settings store owns the
optimistic UI update and rollback. Each webview applies the last-known mirror
before rendering and reconciles from the backend during appearance sync.

`src/styles/fonts.css` declares the four original Libron v0.31 WOFF2 files in
`public/fonts/libron/`, alongside their original copyright and OFL notices.
They ship in the Vite output and are available without a system installation
or runtime network request. Regular covers weights 400–500 and bold covers
600–900; italic variants use the corresponding weight ranges.

`src/lib/typography.ts` resolves role-specific CSS variables. Unknown installed
families are quoted as one literal family name and fall back to Libron;
unsupported glyphs fall back to the operating system's fonts. Generic system
sans, serif, and monospace choices use their respective system stacks.
No font enumeration, background download, or machine-wide installation runs.

Transcript roles cover history (including individual Multi-STT and brain
outputs), live sessions, Live Canvas, overlay streaming text, file results,
and Recall's editor/preview. Code inside Markdown notes still follows the
code/numbers role. Chart axes cache that role's font outside the paint loop
and reuse the appearance observer for changes and font-load completion.

Native tray/context menus and window decorations use platform fonts; Tauri
does not expose custom family styling for those menus. Pasted/exported text is
plain text and retains the destination application's typography. Choosing an
in-app transcript font does not alter transcript contents or rich-text formats.

The font controls have English and French copy; other locales currently use
English fallback copy with complete translation keys.
