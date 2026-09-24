import { describe, expect, it } from "bun:test";
import { displayModelId, resolveModelSetting } from "./modelId";

describe("modelId", () => {
  it("formats display model id correctly", () => {
    expect(
      displayModelId(
        "handy-computer/whisper-large-v3-turbo-gguf/whisper-large-v3-turbo-Q8_0.gguf",
      ),
    ).toBe("whisper-large-v3-turbo-gguf/whisper-large-v3-turbo-Q8_0");
    expect(
      displayModelId("davidxifeng/Confucius4-R2T2-gguf/r2t2-q4_k_m.gguf"),
    ).toBe("davidxifeng/Confucius4-R2T2-gguf/r2t2-q4_k_m");
  });

  describe("resolveModelSetting", () => {
    const base = "davidxifeng/Confucius4-R2T2-gguf";
    const q8 = "davidxifeng/Confucius4-R2T2-gguf/r2t2-q8_0.gguf";
    const q4 = "davidxifeng/Confucius4-R2T2-gguf/r2t2-q4_k_m.gguf";

    it("returns fallback when map or modelId is missing", () => {
      expect(resolveModelSetting(null, q4, "auto")).toBe("auto");
      expect(resolveModelSetting({}, null, "auto")).toBe("auto");
      expect(resolveModelSetting({}, q4, "auto")).toBe("auto");
    });

    it("inherits from base repo to quant variants", () => {
      const map = { [base]: "vulkan_intel" };
      expect(resolveModelSetting(map, q8, "auto")).toBe("vulkan_intel");
      expect(resolveModelSetting(map, q4, "auto")).toBe("vulkan_intel");
    });

    it("inherits from sibling quant to other quants and base", () => {
      const map = { [q8]: "vulkan_nvidia" };
      expect(resolveModelSetting(map, q4, "auto")).toBe("vulkan_nvidia");
      expect(resolveModelSetting(map, base, "auto")).toBe("vulkan_nvidia");
    });

    it("prefers exact match over inherited values", () => {
      const map = {
        [base]: "vulkan_intel",
        [q4]: "cpu",
      };
      expect(resolveModelSetting(map, q4, "auto")).toBe("cpu");
      expect(resolveModelSetting(map, q8, "auto")).toBe("vulkan_intel");
    });
  });
});
