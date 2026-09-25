import { describe, expect, it } from "bun:test";
import { copyToClipboard } from "./clipboard";

describe("copyToClipboard", () => {
  it("returns true on successful writeText", async () => {
    const successfulClipboard = {
      writeText: async () => {},
    };
    expect(await copyToClipboard("copied text", successfulClipboard)).toBe(
      true,
    );
  });

  it("returns false on failed writeText", async () => {
    const failedClipboard = {
      writeText: async () => {
        throw new Error("clipboard unavailable");
      },
    };
    expect(await copyToClipboard("copied text", failedClipboard)).toBe(false);
  });
});
