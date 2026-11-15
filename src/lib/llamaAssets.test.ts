import { describe, expect, test } from "bun:test";
import type { LlamaRelease } from "@/bindings";
import {
  cudaBuildVersion,
  llamaBackendOptions,
  parseCudaVersion,
  pickLlamaAsset,
} from "./llamaAssets";

// The same cases as `llama_releases.rs`'s tests: this file mirrors
// `pick_asset`, which is what the install actually runs.
const release = (backends: string[], tag = "b1"): LlamaRelease => ({
  tag,
  name: tag,
  published_at: "",
  prerelease: true,
  build_number: 1,
  assets: backends.map((backend) => ({
    name: backend,
    size_bytes: 0,
    url: "",
    backend,
  })),
  backing_tag: null,
});

const picked = (
  r: LlamaRelease,
  backend: string,
  driver: [number, number] | null,
) => pickLlamaAsset(r, backend, driver)?.backend ?? null;

describe("llama.cpp asset choice", () => {
  test("parses CUDA build labels and driver versions", () => {
    expect(cudaBuildVersion("cuda-13.5")).toEqual([13, 5]);
    expect(cudaBuildVersion("cuda-13.10")).toEqual([13, 10]);
    expect(cudaBuildVersion("cuda")).toBeNull();
    expect(cudaBuildVersion("cuda-13")).toBeNull();
    expect(cudaBuildVersion("vulkan")).toBeNull();
    expect(parseCudaVersion("13.5")).toEqual([13, 5]);
    expect(parseCudaVersion(null)).toBeNull();
  });

  test("cuda takes the newest build the driver supports", () => {
    const november = release(["cpu", "cuda-12.4", "cuda-13.5", "vulkan"]);
    expect(picked(november, "cuda", [13, 5])).toBe("cuda-13.5");
    expect(picked(november, "cuda", [14, 0])).toBe("cuda-13.5");
    expect(picked(november, "cuda", [13, 4])).toBe("cuda-12.4");
    expect(picked(november, "cuda", null)).toBe("cuda-13.5");
    expect(picked(november, "cuda", [11, 8])).toBe("vulkan");
  });

  test("an exact CUDA build is never substituted", () => {
    const november = release(["cpu", "cuda-12.4", "cuda-13.5", "vulkan"]);
    expect(picked(november, "cuda-12.4", [13, 5])).toBe("cuda-12.4");
    expect(picked(november, "cuda-13.4", [13, 5])).toBeNull();
  });

  test("other backends keep their fallbacks", () => {
    const r = release(["cpu", "vulkan", "cuda-13.5"]);
    expect(picked(r, "vulkan", null)).toBe("vulkan");
    expect(picked(r, "cpu", null)).toBe("cpu");
    expect(picked(release(["cpu"]), "vulkan", null)).toBe("cpu");
    expect(picked(release(["cpu"]), "cuda", [13, 5])).toBe("cpu");
    expect(picked(release([]), "cpu", null)).toBeNull();
  });

  test("offers every CUDA build the releases carry, and the stored choice", () => {
    const releases = [
      release(["cpu", "cuda-12.4", "cuda-13.5", "vulkan"], "b2"),
      release(["cpu", "cuda-12.4", "cuda-13.4", "vulkan"], "b1"),
    ];
    expect(llamaBackendOptions(releases, "auto")).toEqual([
      "auto",
      "cuda",
      "cuda-13.5",
      "cuda-13.4",
      "cuda-12.4",
      "vulkan",
      "cpu",
    ]);
    expect(llamaBackendOptions([], "cuda-13.6")).toEqual([
      "auto",
      "cuda",
      "vulkan",
      "cpu",
      "cuda-13.6",
    ]);
  });
});
