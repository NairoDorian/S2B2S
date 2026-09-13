/**
 * A small Markdown parser for the release-notes modal.
 *
 * Why this is not `react-markdown` any more: that package is a React component
 * library, and Phase 1 of the Solid 2 migration (docs/PLAN_SOLIDJS_2.md) removes
 * every React-only dependency *while the app still runs on React*, so each
 * removal ships on its own instead of riding the framework swap.
 *
 * The subset below is not a guess. It is what
 * `src/content/release-notes/README.md` documents the notes support —
 * "paragraphs, headings, lists, links, code, quotes, separators, hard line
 * breaks, and local images under `/release-notes/...`. Raw HTML is ignored
 * before rendering." — and it is exactly the element list the renderer this
 * replaces whitelisted via `allowedElements`. Nothing here is more permissive
 * than what was there before; the three places it differs are marked
 * DIVERGENCE and all three show *more* text rather than less.
 *
 * Why hand-rolled rather than `marked` or `markdown-it`: those emit an **HTML
 * string**, so the modal would need `dangerouslySetInnerHTML` (Solid:
 * `innerHTML`) plus a sanitiser to be safe at all. Today the entire attack
 * surface is an element whitelist and two URL checks, over files that ship
 * inside the binary. The subset is ~15 constructs, which is fewer lines than
 * the adapter a parser library would need, and it is one more dependency the
 * app does not take on to render its own release notes.
 *
 * Deliberately framework-neutral: no JSX, no React, no Solid, one pure
 * function. Phase 3 rewrites the renderer beside this file and leaves the
 * parser untouched, which is the whole reason the two are separate files.
 */

/** An inline span. `text` and `code` carry strings; the rest nest. */
export type Inline =
  | { type: "text"; value: string }
  | { type: "break" }
  | { type: "strong"; children: Inline[] }
  | { type: "em"; children: Inline[] }
  | { type: "code"; value: string }
  | { type: "link"; href: string; children: Inline[] }
  | { type: "image"; src: string; alt: string };

/**
 * A block of the document. A list's items are inline runs, one per bullet —
 * the notes have no nested lists, see the DIVERGENCE on `parseList`.
 */
export type Block =
  | { type: "heading"; level: 1 | 2 | 3; children: Inline[] }
  | { type: "paragraph"; children: Inline[] }
  | { type: "list"; ordered: boolean; items: Inline[][] }
  | { type: "blockquote"; children: Inline[] }
  | { type: "code"; lang: string; value: string }
  | { type: "hr" };

/**
 * Raw HTML, dropped whole — what the old renderer's `skipHtml` did and what
 * README.md promises. Requiring a tag *shape* rather than any `<` is what keeps
 * prose like "a < b" intact.
 */
const HTML_TAG = /^<\/?[A-Za-z][^>]*>|^<!--[\s\S]*?-->/;

/** Characters a backslash may escape, from CommonMark's ASCII punctuation set. */
const ESCAPABLE = "\\`*_{}[]()#+-.!<>|~";

const FENCE = /^[ \t]{0,3}(`{3,}|~{3,})[ \t]*([^`\s]*)[ \t]*$/;
const FENCE_CLOSE = /^[ \t]{0,3}(`{3,}|~{3,})[ \t]*$/;
const ATX = /^(#{1,6})[ \t]+(.*?)[ \t]*#*[ \t]*$/;
const HR = /^[ \t]{0,3}([-*_])[ \t]*(?:\1[ \t]*){2,}$/;
const QUOTE = /^[ \t]{0,3}>[ \t]?(.*)$/;
const BULLET = /^[ \t]*[-*+][ \t]+(.*)$/;
const ORDERED = /^[ \t]*\d{1,9}[.)][ \t]+(.*)$/;

/** True when a line opens a block rather than continuing the one above it. */
function isBlockStart(line: string): boolean {
  return (
    FENCE.test(line) || ATX.test(line) || HR.test(line) || QUOTE.test(line)
  );
}

/** Up to three leading spaces are padding, not content (CommonMark). */
const stripIndent = (line: string) => line.replace(/^ {0,3}/, "");

/**
 * `**strong**` / `__strong__` / `*em*` / `_em_`, as a token plus the index just
 * past the closing delimiter — or null when the delimiter does not open here.
 *
 * The flanking rules are CommonMark's, simplified: `*` opens only before a
 * non-space, `_` additionally only after a non-word character (which is what
 * keeps `snake_case` and `2 * 3 * 4` literal), and neither closes after a
 * space. No release note relies on them today; they are here so that one which
 * writes either reads as written instead of being mangled.
 */
function emphasis(
  src: string,
  at: number,
): { inline: Inline; end: number } | null {
  const char = src[at];
  const double = src[at + 1] === char;
  const delim = double ? char + char : char;
  const from = at + delim.length;

  if (from >= src.length || /\s/.test(src[from])) return null;
  if (!double && char === "_" && at > 0 && /\w/.test(src[at - 1])) return null;

  for (let i = from; i < src.length; i += 1) {
    if (src[i] !== char) continue;
    if (double && src[i + 1] !== char) continue;
    if (src[i - 1] === "\\") continue;

    const inner = src.slice(from, i);
    if (!inner || /\s$/.test(inner)) continue;

    return {
      inline: double
        ? { type: "strong", children: parseInline(inner) }
        : { type: "em", children: parseInline(inner) },
      end: i + delim.length,
    };
  }

  return null;
}

/** The inline spans of one run of text (a paragraph, a heading, a list item). */
export function parseInline(src: string): Inline[] {
  const out: Inline[] = [];
  let text = "";
  let i = 0;

  const flush = () => {
    if (text) {
      out.push({ type: "text", value: text });
      text = "";
    }
  };

  while (i < src.length) {
    const char = src[i];

    if (char === "\\") {
      const next = src[i + 1];
      if (next === "\n") {
        // A backslash at end of line is a hard break; the other spelling, two
        // trailing spaces, is handled in the newline case below.
        flush();
        out.push({ type: "break" });
        i += 2;
        continue;
      }
      if (next && ESCAPABLE.includes(next)) {
        text += next;
        i += 2;
        continue;
      }
      text += char;
      i += 1;
      continue;
    }

    if (char === "\n") {
      if (/(?: {2,})$/.test(text)) {
        text = text.replace(/ +$/, "");
        flush();
        out.push({ type: "break" });
      } else {
        // A soft break. The parser this replaces left the newline in the text
        // node for the browser to collapse; a space is what that renders as,
        // and it keeps the tokens free of newlines that look significant.
        text += " ";
      }
      i += 1;
      continue;
    }

    if (char === "`") {
      const run = /^`+/.exec(src.slice(i))![0];
      const close = src.indexOf(run, i + run.length);
      if (close !== -1) {
        flush();
        out.push({ type: "code", value: src.slice(i + run.length, close) });
        i = close + run.length;
        continue;
      }
      text += run;
      i += run.length;
      continue;
    }

    if (char === "!" && src[i + 1] === "[") {
      const image = /^!\[([^\]]*)\]\(([^)\s]*)\)/.exec(src.slice(i));
      if (image) {
        flush();
        out.push({ type: "image", alt: image[1], src: image[2] });
        i += image[0].length;
        continue;
      }
    }

    if (char === "[") {
      const link = /^\[([^\]]*)\]\(([^)\s]*)\)/.exec(src.slice(i));
      if (link) {
        flush();
        out.push({
          type: "link",
          href: link[2],
          children: parseInline(link[1]),
        });
        i += link[0].length;
        continue;
      }
    }

    if (char === "*" || char === "_") {
      const span = emphasis(src, i);
      if (span) {
        flush();
        out.push(span.inline);
        i = span.end;
        continue;
      }
    }

    if (char === "<") {
      const html = HTML_TAG.exec(src.slice(i));
      if (html) {
        // Dropped without flushing, so the text either side joins into one
        // token. Only the tags go: their content stays, where a block of
        // markup used to vanish whole (DIVERGENCE, and in the direction of
        // keeping the words).
        i += html[0].length;
        continue;
      }
    }

    text += char;
    i += 1;
  }

  flush();
  return out;
}

/**
 * A fenced block, from the line *after* the opening fence. Returns the body and
 * the index the scanner resumes at. An unterminated fence runs to the end of
 * the document, as it does in CommonMark.
 */
function parseFence(
  lines: string[],
  start: number,
  marker: string,
): { value: string; next: number } {
  const body: string[] = [];
  let i = start;

  while (i < lines.length) {
    const close = FENCE_CLOSE.exec(lines[i]);
    if (
      close &&
      close[1][0] === marker[0] &&
      close[1].length >= marker.length
    ) {
      return { value: body.join("\n"), next: i + 1 };
    }
    body.push(lines[i]);
    i += 1;
  }

  return { value: body.join("\n"), next: i };
}

/**
 * A run of bullets or numbers, starting at `start`.
 *
 * DIVERGENCE: a nested list is flattened into the list around it — a deeper
 * bullet becomes a sibling item, so every word survives and the depth does not.
 * `react-markdown` nested it. No release note nests one.
 */
function parseList(
  lines: string[],
  start: number,
  ordered: boolean,
): { items: Inline[][]; next: number } {
  const marker = ordered ? ORDERED : BULLET;
  const items: Inline[][] = [];
  let current: string[] | null = null;
  let i = start;

  const push = () => {
    if (current) items.push(parseInline(current.join("\n")));
  };

  while (i < lines.length) {
    const line = lines[i];

    if (!line.trim()) {
      // A blank line ends the list, unless the next line is another item —
      // which is how the notes' longer items stay one list.
      const next = lines[i + 1];
      if (next === undefined || !marker.test(next)) break;
      i += 1;
      continue;
    }

    if (marker.test(line)) {
      push();
      current = [marker.exec(line)![1]];
      i += 1;
      continue;
    }

    if (!current || isBlockStart(line)) break;

    // A continuation of the open item — wrapped text, or (CommonMark's lazy
    // continuation) a following line that is not indented and not a block.
    current.push(line.trim());
    i += 1;
  }

  push();
  return { items, next: i };
}

/** Parses a whole release note. */
export function parseMarkdown(markdown: string): Block[] {
  const blocks: Block[] = [];
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    if (!line.trim()) {
      i += 1;
      continue;
    }

    const fence = FENCE.exec(line);
    if (fence) {
      const { value, next } = parseFence(lines, i + 1, fence[1]);
      blocks.push({ type: "code", lang: fence[2] ?? "", value });
      i = next;
      continue;
    }

    const atx = ATX.exec(line);
    if (atx) {
      const level = atx[1].length;
      const children = parseInline(atx[2]);
      // DIVERGENCE: the old renderer allowed h1–h3 only, so a `####` heading
      // and every word under it disappeared. The level is clamped instead — a
      // note that writes one gets a paragraph, not a hole.
      blocks.push(
        level <= 3
          ? { type: "heading", level: level as 1 | 2 | 3, children }
          : { type: "paragraph", children },
      );
      i += 1;
      continue;
    }

    if (HR.test(line)) {
      blocks.push({ type: "hr" });
      i += 1;
      continue;
    }

    if (QUOTE.test(line)) {
      const body: string[] = [];
      while (i < lines.length) {
        const quoted = QUOTE.exec(lines[i]);
        if (!quoted) break;
        body.push(quoted[1]);
        i += 1;
      }
      blocks.push({
        type: "blockquote",
        children: parseInline(body.join("\n")),
      });
      continue;
    }

    if (BULLET.test(line) || ORDERED.test(line)) {
      const ordered = !BULLET.test(line);
      const { items, next } = parseList(lines, i, ordered);
      blocks.push({ type: "list", ordered, items });
      i = next;
      continue;
    }

    const body: string[] = [];
    while (i < lines.length) {
      const text = lines[i];
      if (
        !text.trim() ||
        isBlockStart(text) ||
        BULLET.test(text) ||
        ORDERED.test(text)
      ) {
        break;
      }
      body.push(stripIndent(text));
      i += 1;
    }

    // Unreachable: every line that could leave `body` empty was handled above.
    // It is here so that a future edit to those branches cannot turn this loop
    // into one that never advances.
    if (body.length === 0) {
      i += 1;
      continue;
    }

    blocks.push({ type: "paragraph", children: parseInline(body.join("\n")) });
  }

  return blocks;
}
