// Standalone assert check, run by `bun test` (bunfig.toml roots discovery at
// src/).
//
// Pins the overlay's text reveal. The feed pairs below are verbatim from
// `transcribe-cli --stream-chunk-ms`, one tick per line, for the two families
// whose streaming the overlay serves: R2T2 (qwen3_asr, which re-decodes its
// whole context every chunk and so moves the committed seam most) and Nemotron
// (parakeet cache-aware). The functions under test are the ones the overlay
// calls (src/overlay/RecordingOverlay.tsx), so "the spec holds" is about the
// shipped overlay and not a copy of it.
import { test } from "bun:test";
import assert from "node:assert";
import {
  NO_REVEAL,
  advanceReveal,
  finishReveal,
  joinStreamText,
  revealStride,
  splitReveal,
  type StreamReveal,
} from "./streamReveal";

/** One feed's text, as the backend split it. */
const tick = (committed: string, tentative: string) => ({
  committed,
  tentative,
});

/** Run the reveal to rest against a whole sequence, one tick at a time. */
const runToRest = (
  ticks: { committed: string; tentative: string }[],
  backCorrection = false,
  speed = 30,
): string => {
  let reveal: StreamReveal = NO_REVEAL;
  let target = tick("", "");
  for (const next of ticks) {
    target = next;
    const full = joinStreamText(target.committed, target.tentative);
    // Ticks until the reveal catches up, plus one to observe the resting
    // state — the typewriter stops itself once there is nothing left.
    for (let i = 0; i < full.length + 8; i++) {
      reveal = advanceReveal(
        reveal,
        full,
        revealStride(full.length - reveal.revealed, speed),
        backCorrection,
      );
    }
  }
  const { committed, tentative } = splitReveal(reveal, target.committed.length);
  return committed + tentative;
};

test("the two spans are one text, joined with nothing between them", () => {
  // A real R2T2 tick: the seam falls inside 放射性物质, which has no space to
  // hide a separator behind.
  const r2t2 = tick("日本原子能机构表示，核电站检测出了放射性", "物质碘和碘。");
  assert.strictEqual(
    joinStreamText(r2t2.committed, r2t2.tentative),
    "日本原子能机构表示，核电站检测出了放射性物质碘和碘。",
  );

  // A real Nemotron tick: here the seam falls mid-word with the space carried at
  // the end of the committed half instead ("…to con" + "nect").
  const nemotron = tick("Of course it was impossible to con", "nect the dots");
  assert.strictEqual(
    joinStreamText(nemotron.committed, nemotron.tentative),
    "Of course it was impossible to connect the dots",
  );

  // …and a Nemotron tick whose committed half already ends in the space, where
  // an inserted separator would double it (invisible in HTML, but the text is
  // the text).
  const trailing = tick("…ten years later. ", "Again, you can't connect");
  assert.strictEqual(
    joinStreamText(trailing.committed, trailing.tentative),
    "…ten years later. Again, you can't connect",
  );
});

test("a commit that moves the seam does not show the text twice", () => {
  // The regression. R2T2 commits "Of course, it" while the tail it is
  // committing is still on screen, then the tail is replaced by what follows.
  // Typing the halves independently typed the committed half forward while the
  // stale tail still held the same words, so the sentence opened a second time.
  const ticks = [
    tick("Of course", ", it was impossible to"),
    tick("Of course, it", " was impossible to connect the"),
    tick("Of course, it was", " impossible to connect the dots"),
    tick("Of course, it was impossible", " to connect the dots looking"),
  ];

  let reveal: StreamReveal = NO_REVEAL;
  for (const t of ticks) {
    const full = joinStreamText(t.committed, t.tentative);
    for (let i = 0; i < full.length + 8; i++) {
      reveal = advanceReveal(
        reveal,
        full,
        revealStride(full.length - reveal.revealed, 30),
        false,
      );
      const { committed, tentative } = splitReveal(reveal, t.committed.length);
      const shown = committed + tentative;
      // The invariant everything else follows from: what is on screen is a
      // prefix of the current hypothesis. The old two-halves reveal broke it
      // 209 times over these four ticks — "O , it was impossible to" while the
      // committed half was still typing "Of course" — which is the tail's words
      // showing up before the head's.
      assert.ok(
        full.startsWith(shown),
        `showed "${shown}" which is not a prefix of "${full}"`,
      );
    }
  }
});

test("a whole sequence replays in order, and never moves backwards unasked", () => {
  // The Chinese sample, one tick per line as the CLI printed it.
  const ticks = [
    tick("", "日本原子能机构表示"),
    tick("日本原子能", "机构表示，核电站"),
    tick("日本原子能机构", "表示，核电站检测"),
    tick("日本原子能机构表示", "，核电站检测出了"),
    tick("日本原子能机构表示，核电", "站检测出了放射性"),
    tick("日本原子能机构表示，核电站检测出了放射性", "物质碘和碘。"),
  ];
  assert.strictEqual(
    runToRest(ticks),
    "日本原子能机构表示，核电站检测出了放射性物质碘和碘。",
  );

  // Nemotron's shape: mostly an empty tail, with one tick that has a real one.
  const nemotron = [
    tick("Of course it was", ""),
    tick("Of course it was impossible to con", ""),
    tick("Of course it was impossible to connect the dots looking forward", ""),
    tick(
      "Of course it was impossible to connect the dots looking forward when I was in college",
      "",
    ),
    tick(
      "Of course it was impossible to connect the dots looking forward when I was in college. ",
      "Again, you",
    ),
  ];
  assert.strictEqual(
    runToRest(nemotron),
    "Of course it was impossible to connect the dots looking forward when I was in college. Again, you",
  );
});

test("the seam is the target's, so a mid-word commit can follow the reveal", () => {
  // Fully revealed target, then the seam alone advances — the case where a
  // caught-up reveal still has to re-publish.
  const reveal = finishReveal("hello world");
  assert.deepStrictEqual(splitReveal(reveal, 5), {
    committed: "hello",
    tentative: " world",
  });
  assert.deepStrictEqual(splitReveal(reveal, 9), {
    committed: "hello wor",
    tentative: "ld",
  });
  // The library's own test pins this seam as intended (`committed="turn le"`).
  assert.deepStrictEqual(splitReveal(finishReveal("turn left"), 7), {
    committed: "turn le",
    tentative: "ft",
  });
});

test("a rewrite the reveal has already shown is a block, or a rewind when asked", () => {
  const shown: StreamReveal = { text: "hello world", revealed: 11 };
  // The model rewrote from the 6th character. Back correction off (the
  // default): the new text lands whole, so the preview never moves backwards.
  assert.deepStrictEqual(advanceReveal(shown, "hello there", 2, false), {
    text: "hello there",
    revealed: 11,
  });
  // On: rewind to the shared "hello " and retype the new wording.
  assert.deepStrictEqual(advanceReveal(shown, "hello there", 2, true), {
    text: "hello there",
    revealed: 8,
  });
  // A rewrite entirely in the tail is invisible either way: it is not on screen.
  const behind: StreamReveal = { text: "hello wor", revealed: 6 };
  assert.deepStrictEqual(advanceReveal(behind, "hello worth", 2, true), {
    text: "hello worth",
    revealed: 8,
  });
});

test("a hypothesis that shrinks cannot drive the reveal past its end", () => {
  const shown: StreamReveal = { text: "hello world", revealed: 11 };
  for (const backCorrection of [false, true]) {
    const next = advanceReveal(shown, "hi", 2, backCorrection);
    assert.ok(next.revealed <= next.text.length, "revealed past the end");
    assert.strictEqual(next.text.slice(0, next.revealed), "hi");
  }
});

test("the stride grows with the distance and is never zero", () => {
  assert.strictEqual(revealStride(0, 30), 1);
  assert.strictEqual(revealStride(1, 30), 1);
  assert.strictEqual(revealStride(20, 30), 2);
  assert.strictEqual(revealStride(100, 30), 3);
  // A speed of 0 reads the same as the unset default rather than dividing by it.
  assert.strictEqual(revealStride(100, 0), 3);
});
