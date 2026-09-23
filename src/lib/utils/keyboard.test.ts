// Standalone assert check, run by `bun test` (bunfig.toml roots discovery at
// src/).
import { test } from "bun:test";
import assert from "node:assert";
import { isSimulatableKey } from "./keyboard";

test("named keys and function keys are simulatable", () => {
  assert.ok(isSimulatableKey("space"));
  assert.ok(isSimulatableKey("Ctrl"));
  assert.ok(isSimulatableKey("f1"));
  assert.ok(isSimulatableKey("F24"));
  assert.ok(!isSimulatableKey("f25"));
});

test("single ASCII characters are simulatable", () => {
  assert.ok(isSimulatableKey("a"));
  assert.ok(isSimulatableKey("Z"));
  assert.ok(isSimulatableKey("7"));
  assert.ok(isSimulatableKey(";"));
});

test("non-ASCII and multi-character tokens are not", () => {
  // `input.rs` maps only one-byte tokens, so these would fail to simulate.
  assert.ok(!isSimulatableKey("é"));
  assert.ok(!isSimulatableKey("ß"));
  assert.ok(!isSimulatableKey("ü"));
  assert.ok(!isSimulatableKey("ab"));
  assert.ok(!isSimulatableKey(""));
});
