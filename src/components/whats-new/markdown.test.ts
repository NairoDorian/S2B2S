// Standalone assert check (no JS unit-test runner in this repo). Run with:
//   bun src/components/whats-new/markdown.test.ts
//
// This parser replaced `react-markdown` in Phase 1 of the Solid 2 migration, and
// this file exists for two reasons: to pin the subset the release notes
// document (src/content/release-notes/README.md), and to pin the three places
// the replacement deliberately differs from what it replaced. It runs against
// the notes themselves as well as against samples, because "the files we
// actually ship still parse" is the only claim that has to hold.
import assert from "node:assert";
import { readFileSync } from "node:fs";
import { parseMarkdown, type Block, type Inline } from "./markdown";

const note = (version: string) =>
  readFileSync(
    new URL(`../../content/release-notes/${version}.md`, import.meta.url),
    "utf8",
  );

/** The plain text of an inline run, with all markup stripped. */
const textOf = (nodes: Inline[]): string =>
  nodes
    .map((node): string => {
      switch (node.type) {
        case "text":
        case "code":
          return node.value;
        case "image":
          return node.alt;
        case "break":
          return "\n";
        case "link":
        case "strong":
        case "em":
          return textOf(node.children);
      }
    })
    .join("");

const inlineOf = (source: string): Inline[] =>
  (parseMarkdown(source)[0] as Extract<Block, { type: "paragraph" }>).children;

const inlineTypes = (source: string) =>
  inlineOf(source).map((node) => node.type);

const blockTypes = (source: string) => parseMarkdown(source).map((b) => b.type);

function first<T extends Block["type"]>(
  blocks: Block[],
  type: T,
): Extract<Block, { type: T }> {
  const found = blocks.find((block) => block.type === type);
  assert.ok(found, `expected a ${type} block`);
  return found as Extract<Block, { type: T }>;
}

// --- the notes that ship ----------------------------------------------------

for (const version of ["0.9.0", "0.9.6", "0.9.7"]) {
  const blocks = parseMarkdown(note(version));

  assert.ok(blocks.length > 0, `${version}: parsed to no blocks`);
  for (const block of blocks) {
    // An empty block is a hole in the modal, which is the failure this file
    // exists to catch.
    if (block.type === "hr" || block.type === "code") continue;
    const children =
      block.type === "list" ? block.items.flat() : block.children;
    assert.ok(children.length > 0, `${version}: empty ${block.type}`);
    assert.ok(
      textOf(children).trim().length > 0,
      `${version}: ${block.type} with no text`,
    );
  }
}

const linked = parseMarkdown(note("0.9.0"));
const link = first(linked, "paragraph").children.find((n) => n.type === "link");
assert.ok(link && link.type === "link", "0.9.0: the transcribe.cpp link");
assert.equal(link.href, "https://github.com/handy-computer/transcribe.cpp");
assert.equal(textOf(link.children), "transcribe.cpp");

const captioned = parseMarkdown(note("0.9.0")).find(
  (block) =>
    block.type === "paragraph" &&
    block.children.some((node) => node.type === "image"),
);
assert.ok(captioned && captioned.type === "paragraph", "0.9.0: the screenshot");
const img = captioned.children.find((node) => node.type === "image");
assert.ok(img && img.type === "image");
assert.equal(img.src, "/release-notes/0.9.0/overlay.png");
assert.equal(img.alt, "Streaming transcription live overlay preview");

// Marks that would be mangled if any delimiter rule were too eager: the
// multi-STT merge placeholders sit inside code spans, and every note is full
// of `**strong**` and of em dashes.
const multiStt = parseMarkdown(note("0.9.6"));
assert.ok(
  JSON.stringify(multiStt).includes('"${output2}"'),
  "0.9.6: the ${output2} code span",
);
assert.ok(
  JSON.stringify(multiStt).includes('"Multi-STT"'),
  "0.9.6: **Multi-STT** as strong, not as a mangled em",
);

// --- the documented subset --------------------------------------------------

assert.deepEqual(blockTypes("# One\n\n## Two\n\n### Three"), [
  "heading",
  "heading",
  "heading",
]);
assert.deepEqual(
  [1, 2, 3].map(
    (n) => first(parseMarkdown(`${"#".repeat(n)} H`), "heading").level,
  ),
  [1, 2, 3],
);

assert.deepEqual(blockTypes("- a\n- b"), ["list"]);
assert.equal(first(parseMarkdown("- a"), "list").ordered, false);
assert.equal(first(parseMarkdown("1. a\n2. b"), "list").ordered, true);
assert.equal(first(parseMarkdown("- a"), "list").items.length, 1);

// A wrapped continuation is the same item, and a blank line before the next
// bullet does not split the list in two.
const list = first(parseMarkdown("- one\n  continued\n\n- two"), "list");
assert.deepEqual(list.items.map(textOf), ["one continued", "two"]);

// CommonMark's lazy continuation: an unindented line after an item continues
// it rather than starting a paragraph.
assert.deepEqual(first(parseMarkdown("- foo\nbar"), "list").items.map(textOf), [
  "foo bar",
]);

assert.deepEqual(inlineTypes("a **b** c"), ["text", "strong", "text"]);
assert.deepEqual(inlineTypes("a __b__ c"), ["text", "strong", "text"]);
assert.deepEqual(inlineTypes("`x` and *y*"), ["code", "text", "em"]);
assert.deepEqual(inlineTypes("a _b_ c"), ["text", "em", "text"]);
assert.deepEqual(inlineOf("![alt](/release-notes/1/a.png)"), [
  { type: "image", src: "/release-notes/1/a.png", alt: "alt" },
]);
assert.deepEqual(inlineOf("[text](https://example.com/a)"), [
  {
    type: "link",
    href: "https://example.com/a",
    children: [{ type: "text", value: "text" }],
  },
]);

assert.deepEqual(parseMarkdown("```text\nx = 1\n```"), [
  { type: "code", lang: "text", value: "x = 1" },
]);
assert.deepEqual(parseMarkdown("```\nx\n```"), [
  { type: "code", lang: "", value: "x" },
]);
assert.deepEqual(blockTypes("a\n\n---\n\nb"), ["paragraph", "hr", "paragraph"]);
assert.deepEqual(
  first(parseMarkdown("> quoted\n> more"), "blockquote").children,
  [{ type: "text", value: "quoted more" }],
);

// Two trailing spaces are a hard break; a soft break renders as a space.
assert.deepEqual(inlineTypes("a  \nb"), ["text", "break", "text"]);
assert.equal(textOf(inlineOf("a\nb")), "a b");

// --- what must NOT become markup --------------------------------------------

assert.equal(textOf(inlineOf("snake_case_name")), "snake_case_name");
assert.equal(textOf(inlineOf("2 * 3 * 4")), "2 * 3 * 4");
assert.equal(textOf(inlineOf("a \\* b")), "a * b");
assert.equal(textOf(inlineOf("2 < 3 and 4 > 3")), "2 < 3 and 4 > 3");
assert.deepEqual(inlineOf("####### seven"), [
  { type: "text", value: "####### seven" },
]);
assert.deepEqual(parseMarkdown(""), []);
assert.deepEqual(parseMarkdown("\n\n\n"), []);

// --- the three deliberate divergences ---------------------------------------

// 1. h4+ keeps its text rather than losing the whole subtree.
assert.deepEqual(blockTypes("#### Four"), ["paragraph"]);
assert.equal(textOf(inlineOf("#### Four")), "Four");

// 2. A nested list flattens: the items survive, the depth does not.
assert.equal(first(parseMarkdown("- outer\n- inner"), "list").items.length, 2);
assert.equal(
  first(parseMarkdown("- outer\n  - inner"), "list").items.length,
  2,
);

// 3. Raw HTML: the tags go, the words stay.
assert.equal(textOf(inlineOf("a <b>bold</b> c")), "a bold c");

console.log("markdown: all assertions passed");
