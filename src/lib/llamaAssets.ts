import type { LlamaRelease, LlamaReleaseAsset } from "@/bindings";

/**
 * The Local LLM page's preview of which llama.cpp build an install will take.
 * It mirrors `llama_releases::pick_asset` (Rust), which is what the install
 * actually runs; `llamaAssets.test.ts` holds the same cases as the Rust tests,
 * so the two cannot drift apart unnoticed.
 *
 * The build is read from the release's own file list, never from a fixed list
 * of versions: llama.cpp moves its CUDA builds (13.4 → 13.5), and a fixed list
 * once installed a CUDA 12 build that a CUDA 13 toolkit cannot run.
 */

/** `[major, minor]`, compared lexicographically. */
export type CudaVersion = readonly [number, number];

/** `cuda-13.5` → `[13, 5]`; `null` for any other label. */
export function cudaBuildVersion(label: string): CudaVersion | null {
  const match = /^cuda-(\d+)\.(\d+)$/.exec(label);
  return match ? [Number(match[1]), Number(match[2])] : null;
}

/** `"13.5"` (as `detect_llama_backend` reports the driver) → `[13, 5]`. */
export function parseCudaVersion(
  text: string | null | undefined,
): CudaVersion | null {
  const match = text ? /^(\d+)\.(\d+)$/.exec(text.trim()) : null;
  return match ? [Number(match[1]), Number(match[2])] : null;
}

const atMost = (a: CudaVersion, b: CudaVersion) =>
  a[0] < b[0] || (a[0] === b[0] && a[1] <= b[1]);

/** The release's CUDA builds, newest first. */
export function cudaAssets(
  release: LlamaRelease,
): { asset: LlamaReleaseAsset; version: CudaVersion }[] {
  return release.assets
    .flatMap((asset) => {
      const version = cudaBuildVersion(asset.backend);
      return version ? [{ asset, version }] : [];
    })
    .toSorted((x, y) =>
      y.version[0] !== x.version[0]
        ? y.version[0] - x.version[0]
        : y.version[1] - x.version[1],
    );
}

/**
 * What `pick_asset` installs for `backend` (`auto` already resolved to `cuda`
 * or `cpu`), or `null` where it reports an error.
 */
export function pickLlamaAsset(
  release: LlamaRelease,
  backend: string,
  driver: CudaVersion | null,
): LlamaReleaseAsset | null {
  const named = (b: string) =>
    release.assets.find((asset) => asset.backend === b) ?? null;
  const firstOf = (chain: string[]) =>
    chain.map(named).find((asset) => asset !== null) ?? null;
  if (backend === "cuda") {
    const runnable = cudaAssets(release).find(
      ({ version }) => driver === null || atMost(version, driver),
    );
    return runnable?.asset ?? firstOf(["vulkan", "cpu"]);
  }
  if (cudaBuildVersion(backend)) return named(backend);
  if (backend === "vulkan") return firstOf(["vulkan", "cpu"]);
  if (backend === "cpu") return named("cpu");
  return firstOf([backend, "vulkan", "cpu"]);
}

/**
 * The backend choices to offer: `auto`, `cuda` (newest the driver supports),
 * every exact CUDA build the loaded releases carry (newest first), `vulkan`,
 * `cpu` — plus the stored value when no loaded release has it, so a saved
 * choice is never silently shown as something else.
 */
export function llamaBackendOptions(
  releases: LlamaRelease[],
  current: string,
): string[] {
  const exact = new Map<string, CudaVersion>();
  for (const release of releases)
    for (const { asset, version } of cudaAssets(release))
      exact.set(asset.backend, version);
  const builds = [...exact.entries()]
    .toSorted(([, x], [, y]) => (x[0] !== y[0] ? y[0] - x[0] : y[1] - x[1]))
    .map(([label]) => label);
  const options = ["auto", "cuda", ...builds, "vulkan", "cpu"];
  return options.includes(current) ? options : [...options, current];
}
