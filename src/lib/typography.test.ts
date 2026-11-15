import { describe, expect, test } from "bun:test";
import {
  DEFAULT_TYPOGRAPHY,
  fontStack,
  normalizeTypography,
} from "./typography";

describe("font preferences", () => {
  test("old, partial and malformed preferences retain safe defaults", () => {
    expect(normalizeTypography(null)).toEqual(DEFAULT_TYPOGRAPHY);
    expect(
      normalizeTypography({ ui: " Verdana ", transcript: "Georgia" }),
    ).toEqual({ ...DEFAULT_TYPOGRAPHY, ui: "Verdana", transcript: "Georgia" });
    expect(
      normalizeTypography({
        ui: "inherit",
        menus: "\n",
        overlay: 42,
        transcript: "x".repeat(129),
        mono: "bad\u0085font",
      }),
    ).toEqual(DEFAULT_TYPOGRAPHY);
  });

  test("font names remain quoted CSS data, with an offline fallback", () => {
    expect(fontStack('A "quoted" \\ font')).toStartWith(
      '"A \\"quoted\\" \\\\ font", "Libron"',
    );
    expect(fontStack("Missing Font")).toContain('"Libron"');
    expect(fontStack("inherit", "Georgia")).toBe(fontStack("Georgia"));
    expect(fontStack("system-mono")).toEndWith("monospace");
  });
});
