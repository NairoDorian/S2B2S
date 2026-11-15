import { expect, test } from "bun:test";
import {
  CanvasTranscript,
  stepCanvasText,
  canvasCorrection,
} from "./liveCanvasTranscript";

test("live tentative words and corrections arrive immediately", () => {
  const text = new CanvasTranscript();
  expect(text.stream("", "wrong words")).toBe("wrong words");
  expect(text.stream("", "correct words")).toBe("correct words");
  expect(text.stream("correct wor", "ds coming")).toBe("correct words coming");
  expect(text.stream("correct words ", "changing")).toBe(
    "correct words changing",
  );
});

test("final chunks retain their tails once and later chunks append", () => {
  const text = new CanvasTranscript();
  text.stream("First ", "thought");
  expect(text.finish("First thought.\n")).toBe("First thought.\n");
  expect(text.finish("First thought.\n")).toBe("First thought.\n");
  expect(text.stream("", "")).toBe("First thought.\n");
  expect(text.stream("Second ", "thought")).toBe(
    "First thought.\nSecond thought",
  );
  expect(text.finish("Second thought.\n")).toBe(
    "First thought.\nSecond thought.\n",
  );
});

test("corrections preserve shared characters without replaying appended words", () => {
  expect(canvasCorrection("The cat", "The dog")).toEqual({
    corrected: "The dog",
    oldChanged: [4, 5, 6],
    newChanged: [4, 5, 6],
  });
  expect(
    canvasCorrection("The cat sat", "The small cat sat again")?.corrected,
  ).toBe("The small cat sat");
  expect(canvasCorrection("The cat", "The dog runs")?.corrected).toBe(
    "The dog",
  );
  expect(canvasCorrection("colour", "color")).toEqual({
    corrected: "color",
    oldChanged: [4],
    newChanged: [],
  });
  expect(canvasCorrection("A😀 tail", "A😎 tail")).toEqual({
    corrected: "A😎 tail",
    oldChanged: [1],
    newChanged: [1],
  });
  expect(canvasCorrection("new", "new words")).toBeNull();
  expect(stepCanvasText("A", "A😀")).toEqual({ next: "A😀", letter: "😀" });
  expect(stepCanvasText("wrong", "correct")).toBeNull();
});
