/**
 * The overlay's text reveal, as a pure state machine — the functions
 * `RecordingOverlay.tsx` actually calls, so "the spec holds" is about the
 * shipped overlay and not a copy of it.
 *
 * The stream hands the overlay one text cut in two: `committed` for the part
 * the model will not rewrite, `tentative` for the volatile tail. The two are
 * one string. The library's own render is `committed + tentative` byte for
 * byte — `include/transcribe.h` says so ("display_text = committed_text +
 * tentative_text"), every binding implements exactly that, and the spaces are
 * already inside one half or the other. The seam between them is not a word
 * boundary either: it is wherever the model's stable-prefix rule cut, which for
 * R2T2 is a byte offset derived from holding back five tokens (`turn le` +
 * `ft` is a case the library's own tests pin as intended), and for Nemotron is
 * usually the end of the hypothesis.
 *
 * So the reveal grows ONE string, not two. That is what keeps the text in
 * order: a model moves text out of the tail into the committed part as it firms
 * up, so the halves change independently, and a reveal that tracked them
 * separately showed the same words twice — the committed half typed forward
 * while the stale tail still held those words behind it ("…was impossible" and
 * then ", it was impossible…" again). With one string no character can be on
 * screen in two places, and the two spans are re-derived from the target on
 * every read rather than accumulated.
 */

export interface StreamReveal {
  /** The hypothesis the reveal is a prefix of. */
  text: string;
  /** How many characters of it are on screen. */
  revealed: number;
}

export const NO_REVEAL: StreamReveal = { text: "", revealed: 0 };

/** The whole of what the backend sent, joined the way the library joins it. */
export const joinStreamText = (committed: string, tentative: string): string =>
  committed + tentative;

/**
 * How many characters one tick reveals. The reveal runs ahead of the speaker at
 * the overlay's own speed setting, with a longer stride while it is behind by
 * more than a line or so, so a burst of text does not take seconds to appear.
 * `speed` is `overlay_direct_speed`.
 */
export const revealStride = (remaining: number, speed: number): number => {
  const threshold = Math.max(15, Math.round((speed || 30) * 0.6));
  return remaining > threshold * 2 ? 3 : remaining > threshold ? 2 : 1;
};

/**
 * Advance the reveal towards `full`, the target's whole text.
 *
 * The common case is an extension — a longer tail, or the same tail with more
 * of its head committed — and the reveal simply grows. When the model has
 * rewritten text the reveal already covers, `backCorrection` decides what the
 * viewer sees: on, the reveal rewinds to the last shared character and retypes
 * the new wording, a visible correction; off, the new text lands as one block,
 * so the preview never moves backwards and a model that revises on every chunk
 * cannot make the text stutter.
 */
export function advanceReveal(
  state: StreamReveal,
  full: string,
  stride: number,
  backCorrection: boolean,
): StreamReveal {
  const shown = state.text.slice(0, state.revealed);
  if (shown === full) return state;

  if (full.startsWith(shown)) {
    return {
      text: full,
      revealed: Math.min(full.length, state.revealed + stride),
    };
  }

  // `full` does not extend what is shown, so text on screen was rewritten.
  if (!backCorrection) return finishReveal(full);

  let common = 0;
  while (
    common < shown.length &&
    common < full.length &&
    shown[common] === full[common]
  ) {
    common++;
  }
  return { text: full, revealed: Math.min(full.length, common + stride) };
}

/** Show all of `full` at once — the flush and the block-replace paths. */
export const finishReveal = (full: string): StreamReveal => ({
  text: full,
  revealed: full.length,
});

/**
 * The two spans the template renders. The seam is a property of the target and
 * not of the reveal, so a commit that lands inside a word only changes which
 * span a character is in, and never regroups the characters themselves.
 */
export function splitReveal(
  state: StreamReveal,
  committedLength: number,
): { committed: string; tentative: string } {
  const shown = state.text.slice(0, state.revealed);
  const cut = Math.min(committedLength, shown.length);
  return { committed: shown.slice(0, cut), tentative: shown.slice(cut) };
}
