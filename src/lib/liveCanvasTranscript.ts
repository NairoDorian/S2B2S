/** One visible edit per step, including Unicode code points. */
export function stepCanvasText(shown: string, target: string) {
  if (shown === target || !target.startsWith(shown)) return null;
  const letter = Array.from(target.slice(shown.length))[0];
  return { letter, next: shown + letter };
}

/** Compare only the revised region; shared prefix/suffix glyphs stay visible. */
export function canvasCorrection(shown: string, target: string) {
  if (target.startsWith(shown)) return null;
  const old = Array.from(shown);
  let corrected = target;
  // When the end word is revised and new words arrive in the same update,
  // retain the old word count for the morph, then reveal the appended words.
  const oldWords = shown.match(/\S+/g)?.length ?? 0;
  const words = [...target.matchAll(/\S+/g)];
  if (words.length > oldWords && oldWords > 0) {
    const oldLast = shown.match(/\S+/g)!.at(-1)!;
    const last =
      words.findLast((word) => word[0] === oldLast) ?? words[oldWords - 1];
    const candidate = target.slice(0, last.index! + last[0].length);
    corrected = candidate;
  }
  const next = Array.from(corrected);
  let prefix = 0;
  while (
    prefix < Math.min(old.length, next.length) &&
    old[prefix] === next[prefix]
  )
    prefix++;
  let suffix = 0;
  while (
    suffix < Math.min(old.length, next.length) - prefix &&
    old[old.length - 1 - suffix] === next[next.length - 1 - suffix]
  )
    suffix++;
  const oldChanged: number[] = [],
    newChanged: number[] = [];
  const m = old.length - suffix - prefix,
    n = next.length - suffix - prefix;
  const oldSame = new Set<number>(),
    newSame = new Set<number>();
  // Bound the character comparison to 65k cells in the changed region.
  if ((m + 1) * (n + 1) <= 65536) {
    const table = new Uint16Array((m + 1) * (n + 1));
    const at = (i: number, j: number) => i * (n + 1) + j;
    for (let i = m - 1; i >= 0; i--)
      for (let j = n - 1; j >= 0; j--)
        table[at(i, j)] =
          old[prefix + i] === next[prefix + j]
            ? 1 + table[at(i + 1, j + 1)]
            : Math.max(table[at(i + 1, j)], table[at(i, j + 1)]);
    let i = 0,
      j = 0;
    while (i < m && j < n) {
      if (old[prefix + i] === next[prefix + j]) {
        oldSame.add(prefix + i++);
        newSame.add(prefix + j++);
      } else if (table[at(i + 1, j)] >= table[at(i, j + 1)]) i++;
      else j++;
    }
  } else {
    for (let i = prefix; i < Math.min(old.length, next.length) - suffix; i++)
      if (old[i] === next[i]) {
        oldSame.add(i);
        newSame.add(i);
      }
  }
  for (let i = prefix; i < old.length - suffix; i++)
    if (!oldSame.has(i)) oldChanged.push(i);
  for (let i = prefix; i < next.length - suffix; i++)
    if (!newSame.has(i)) newChanged.push(i);
  return { corrected, oldChanged, newChanged };
}

/** A raw chunk is finalized separately from the next chunk's stream. */
export class CanvasTranscript {
  private base = "";
  private current = "";
  private finalized = false;

  stream(committed: string, tentative: string): string {
    if (!committed && !tentative) {
      this.current = "";
      return this.base;
    }
    if (this.finalized && (committed || tentative)) this.finalized = false;
    if (!this.finalized) {
      this.current = committed + tentative;
    }
    return this.base + this.current;
  }

  finish(text: string): string {
    if (!this.finalized) {
      this.base += text;
      this.current = "";
      this.finalized = true;
    }
    return this.base;
  }
}
