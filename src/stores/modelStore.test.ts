// Standalone assert check (no JS unit-test runner in this repo). Run with:
//   bun src/stores/modelStore.test.ts
//
// This file pins the two things about `modelStore.ts` that the browser suite
// cannot reach: its `get_available_models` fixture is the empty list, so the
// backend merge — the one piece of this store with real logic in it — never
// runs on real data there.
//
// It pins, first, the *transitions* the store performs (what the merge adds,
// keeps and drops; what a download seeds and a failure clears; what a cancel
// removes). And second, the property the store inherited from the old `immer`
// implementation and must not lose: **structural sharing**. A consumer that
// selected `downloadStats[someModel]` must not be handed a new reference
// because a *different* model's entry changed, or every progress event
// re-renders every download row in the UI.
//
// How identity is asserted now. The store is a Solid store: reading
// `store.downloadProgress` returns a stable proxy for as long as the property
// is not reassigned, so "the merge did not replace this map" is
// `strictEqual(store.downloadProgress, capturedBefore)` — and a *replacement*
// fails it. Comparing against the raw object that was seeded would not work,
// because the store wraps values in its own proxy on the way in.
//
// The event listeners are deliberately not covered: they are registered inside
// `initialize()`, which calls `listen()` from `@tauri-apps/api/event`, and the
// events they handle are exercised end-to-end by the Playwright suite instead.
import assert from "node:assert";
import type { ModelInfo } from "@/bindings";
import { commands } from "@/bindings";
import {
  cancelDownload,
  downloadModel,
  loadModels,
  useModelStore,
} from "./modelStore";

// Every action here is driven straight, so `initialize()` never runs and the
// backend is only reached through the three commands the actions call.
const store = useModelStore();

const progressFor = (id: string, downloaded = 0) => ({
  model_id: id,
  downloaded,
  total: 100,
  percentage: downloaded,
});

const statsFor = (id: string, speed = 1) => ({
  startTime: 0,
  lastUpdate: 0,
  totalDownloaded: 0,
  speed,
});

/** The store reads `id` and `is_downloading` on this path and nothing else. */
const model = (id: string, is_downloading = false) =>
  ({ id, is_downloading }) as unknown as ModelInfo;

/** A plain snapshot of a store map, for `deepStrictEqual` against a literal. */
const plain = <T extends object>(value: T): T => ({ ...value });

// ---- 1. the backend merge: add what it reports, keep what has progress ----
{
  store.setModels([]);
  store.setDownloadingModels({ kept: true, stale: true });
  store.setDownloadProgress({ kept: progressFor("kept") });
  store.setDownloadStats({ kept: statsFor("kept") });

  const progressBefore = store.downloadProgress;
  const statsBefore = store.downloadStats;

  commands.getAvailableModels = async () => ({
    status: "ok",
    data: [model("fresh", true), model("idle")],
  });
  await loadModels();

  assert.deepStrictEqual(
    plain(store.downloadingModels),
    { kept: true, fresh: true },
    "`fresh` is added from the backend, `kept` survives on its progress, and " +
      "`stale` — neither reported nor in progress — is dropped",
  );

  // The maps the merge did not touch must not be replaced. This is the
  // structural-sharing guarantee the old `immer` `produce` provided, and the
  // reason the merge writes only the one map it touches.
  assert.strictEqual(
    store.downloadProgress,
    progressBefore,
    "an untouched map keeps its identity across a merge",
  );
  assert.strictEqual(
    store.downloadStats,
    statsBefore,
    "an untouched map keeps its identity across a merge",
  );
  console.log("ok 1 — loadModels merge");
}

// ---- 2. a no-op write is not a new reference ------------------------------
{
  store.setDownloadingModels({ a: true });
  store.setVerifyingModels({ b: true });

  const downloadingBefore = store.downloadingModels;
  const verifyingBefore = store.verifyingModels;

  commands.getAvailableModels = async () => ({
    status: "ok",
    data: [model("a", true)],
  });
  await loadModels();

  assert.strictEqual(
    store.downloadingModels,
    downloadingBefore,
    "re-reporting a model that is already marked downloading changes nothing",
  );
  assert.strictEqual(
    store.verifyingModels,
    verifyingBefore,
    "the merge does not touch `verifyingModels` at all",
  );
  console.log("ok 2 — no-op writes keep identity");
}

// ---- 3. downloadModel seeds both maps, and a failure clears them ----------
{
  store.setDownloadingModels({ other: true });
  store.setVerifyingModels({ other: true });
  store.setDownloadProgress({ other: progressFor("other") });
  store.setDownloadStats({ other: statsFor("other") });

  commands.downloadModel = async () => ({ status: "ok", data: null });
  await downloadModel("x");

  assert.strictEqual(store.downloadingModels.x, true);
  assert.deepStrictEqual(
    plain(store.downloadProgress.x),
    { model_id: "x", downloaded: 0, total: 0, percentage: 0 },
    "a download starts at zero rather than at whatever the last one left",
  );
  assert.strictEqual(
    store.verifyingModels.other,
    true,
    "another model's verification is not disturbed by this one starting",
  );

  // The command's own failure path — the fallback for an event that never
  // arrives. The state is reset first: a *successful* download deliberately
  // keeps its progress entry, and only the completion event clears it.
  store.setDownloadingModels({ other: true });
  store.setVerifyingModels({ x: true, other: true });
  store.setDownloadProgress({ other: progressFor("other") });
  store.setDownloadStats({ other: statsFor("other") });
  commands.downloadModel = async () => ({ status: "error", error: "nope" });
  assert.strictEqual(await downloadModel("y"), false);
  assert.deepStrictEqual(
    plain(store.downloadProgress),
    { other: progressFor("other") },
    "a failed download leaves no progress entry behind",
  );
  assert.strictEqual(
    store.downloadingModels.y,
    undefined,
    "a failed download leaves no spinner behind",
  );
  assert.strictEqual(
    store.verifyingModels.x,
    true,
    "the failure path leaves a verification the backend is still running " +
      "alone — the cleanup is called without `verifying` there",
  );
  console.log("ok 3 — downloadModel seeds and cleans up");
}

// ---- 4. cancelDownload clears the model and keeps the rest ----------------
{
  store.setDownloadingModels({ x: true, other: true });
  store.setVerifyingModels({ x: true, other: true });
  store.setDownloadProgress({
    x: progressFor("x", 40),
    other: progressFor("other"),
  });
  store.setDownloadStats({ x: statsFor("x"), other: statsFor("other") });

  commands.cancelDownload = async () => ({ status: "ok", data: null });
  commands.getAvailableModels = async () => ({ status: "ok", data: [] });
  assert.strictEqual(await cancelDownload("x"), true);

  assert.strictEqual(store.downloadingModels.x, undefined);
  assert.strictEqual(store.downloadProgress.x, undefined);
  assert.strictEqual(store.downloadStats.x, undefined);
  assert.deepStrictEqual(
    plain(store.downloadingModels),
    { other: true },
    "cancelling one model leaves the others downloading",
  );
  assert.strictEqual(store.downloadStats.other.speed, 1);
  console.log("ok 4 — cancelDownload");
}

// ---- 5. clearing a model that was never downloading touches nothing -------
//
// The "delete an absent key" half of the identity guard, which check 2 (the
// "re-add a present key" half) cannot reach: `loadModels` never deletes a key
// that is present, because a model can only be in `downloadingModels` if the
// backend reported it or progress exists for it. Cancelling a model that is
// not downloading is the shortest path to it — a no-op in every one of the
// three maps the cleanup writes.
{
  store.setDownloadingModels({});
  store.setVerifyingModels({});
  store.setDownloadProgress({ other: progressFor("other") });
  store.setDownloadStats({ other: statsFor("other") });

  const downloadingBefore = store.downloadingModels;
  const progressBefore = store.downloadProgress;
  const statsBefore = store.downloadStats;

  commands.cancelDownload = async () => ({ status: "ok", data: null });
  commands.getAvailableModels = async () => ({ status: "ok", data: [] });
  assert.strictEqual(await cancelDownload("never"), true);

  assert.strictEqual(
    store.downloadProgress,
    progressBefore,
    "clearing an absent key must not replace the map it is absent from",
  );
  assert.strictEqual(
    store.downloadStats,
    statsBefore,
    "clearing an absent key must not replace the map it is absent from",
  );
  assert.strictEqual(
    store.downloadingModels,
    downloadingBefore,
    "clearing an absent key must not replace the map it is absent from",
  );
  console.log("ok 5 — clearing an absent key is a no-op");
}

console.log("\n5 checks passed");
