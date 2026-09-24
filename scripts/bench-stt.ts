// Installed-model replay through the app's headless transcription entry point.
// Exactly three runs share one loaded model. Run 1 is excluded from all scores.
import { parseArgs } from "node:util";
import { resolve, dirname } from "node:path";
import {
  mkdirSync,
  openSync,
  closeSync,
  readFileSync,
  writeFileSync,
  readdirSync,
} from "node:fs";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";

const { values } = parseArgs({
  options: {
    exe: { type: "string" },
    wav: { type: "string" },
    output: { type: "string" },
    model: { type: "string", multiple: true },
    baseline: { type: "string" },
    timeout: { type: "string", default: "600" },
    "max-regression": { type: "string", default: "0.15" },
    backend: { type: "string", multiple: true },
  },
});
if (!values.exe || !values.wav || !values.output) {
  throw new Error(
    "Usage: bun scripts/bench-stt.ts --exe <app executable> --wav <WAV> --output <directory> [--model <installed id>] [--baseline <summary.json>]",
  );
}
const exe = resolve(values.exe);
const wav = resolve(values.wav);
const wavSha256 = createHash("sha256").update(readFileSync(wav)).digest("hex");
const executableSha256 = createHash("sha256")
  .update(readFileSync(exe))
  .digest("hex");
const runtimeFiles = Object.fromEntries(
  readdirSync(dirname(exe))
    .filter((name) => /^(lib)?transcribe.*\.(dll|so|dylib)$/.test(name))
    .map((name) => [
      name,
      createHash("sha256")
        .update(readFileSync(resolve(dirname(exe), name)))
        .digest("hex"),
    ]),
);
if (!Number.isFinite(Number(values.timeout)) || Number(values.timeout) <= 0)
  throw new Error("Timeout must be a positive number");
if (
  !Number.isFinite(Number(values["max-regression"])) ||
  Number(values["max-regression"]) < 0
)
  throw new Error("Regression budget must be nonnegative");
const directory = resolve(values.output);
mkdirSync(directory, { recursive: true });

async function run(args: string[], name: string): Promise<string> {
  const fd = openSync(resolve(directory, `${name}.log`), "w");
  try {
    return await new Promise((resolveRun, reject) => {
      const child = spawn(exe, args, {
        windowsHide: true,
        stdio: ["ignore", "pipe", fd],
      });
      const chunks: Buffer[] = [];
      child.stdout!.on("data", (chunk: Buffer) => chunks.push(chunk));
      const timer = setTimeout(
        () => {
          child.kill();
          reject(new Error(`${name}: timed out`));
        },
        Number(values.timeout) * 1000,
      );
      child.on("error", (error) => {
        clearTimeout(timer);
        reject(error);
      });
      child.on("close", (code) => {
        clearTimeout(timer);
        if (code === 0) resolveRun(Buffer.concat(chunks).toString("utf8"));
        else reject(new Error(`${name}: exit ${code}; see log`));
      });
    });
  } finally {
    closeSync(fd);
  }
}

// --- native per-chunk stage metrics -----------------------------------------
//
// An instrumented native library emits one line per streaming chunk from
// emit_streaming_chunk (src/arch/parakeet/model.cpp):
//
//   parakeet stream chunk 7: total=12.3 ms  graph_build=0.4 ms  ...
//   ... (backend=CUDA0, threads=6, T_q=17, T_cache=70, kv_mode=1, n_layers=24)
//
// The app replays all three repetitions inside one process, so the log holds
// every run's chunks back to back and carries no explicit run marker. The
// per-chunk index is the boundary: each run restarts it at 0. Groups are
// therefore split on "chunk 0", and the first is dropped as warm-up so stage
// metrics follow the same policy as the wall clock above. A library without
// the instrumentation emits nothing and the report simply leaves the columns
// blank, which is why this parses a log instead of requiring a new ABI.
const STAGES = [
  "total",
  "graph_build",
  "sched_alloc",
  "graph_compute",
  "readback",
  "cache_rot",
  "decoder",
  "other",
] as const;
const STAGE_LINE =
  /^parakeet stream chunk (\d+): total=([\d.]+) ms\s+graph_build=([\d.]+) ms\s+sched_alloc=([\d.]+) ms\s+graph_compute=([\d.]+) ms\s+readback=([\d.]+) ms\s+cache_rot=([\d.]+) ms\s+decoder=([\d.]+) ms\s+other=([\d.]+) ms\s+\(backend=([^,]+), threads=(\d+), T_q=(-?\d+), T_cache=(-?\d+), kv_mode=(\d+), n_layers=(\d+)\)/;

// Same order statistic the native pipeline uses, so the two sides are directly
// comparable rather than merely similar.
function percentile(sorted: number[], quantile: number): number {
  return sorted[
    Math.min(sorted.length - 1, Math.floor((sorted.length - 1) * quantile))
  ];
}

function stageMetrics(
  logPath: string,
  expectedRuns: number,
): Record<string, unknown> | null {
  const groups: Record<string, number>[][] = [];
  let geometry: Record<string, unknown> | null = null;
  let current: Record<string, number>[] | null = null;
  for (const line of readFileSync(logPath, "utf8").split(/\r?\n/)) {
    const match = line.match(STAGE_LINE);
    if (!match) continue;
    if (match[1] === "0") {
      current = [];
      groups.push(current);
    }
    if (!current) continue; // chunks logged before any observed run boundary
    current.push(
      Object.fromEntries(
        STAGES.map((stage, i) => [stage, Number(match[i + 2])] as const),
      ),
    );
    geometry = {
      backend: match[10].trim(),
      threads: Number(match[11]),
      T_q: Number(match[12]),
      T_cache: Number(match[13]),
      kv_mode: Number(match[14]),
      n_layers: Number(match[15]),
    };
  }
  const rows = groups.slice(1, expectedRuns).flat();
  if (!rows.length) return null;
  return {
    source: "app log scrape of the native chunk records (emit_streaming_chunk)",
    chunks: rows.length,
    geometry,
    chunks_per_run: Object.fromEntries(
      groups.map((group, index) => [`run${index + 1}`, group.length] as const),
    ),
    stages_ms: Object.fromEntries(
      STAGES.map((stage) => {
        const values = rows.map((row) => row[stage]);
        const sorted = [...values].sort((a, b) => a - b);
        return [
          stage,
          {
            mean: values.reduce((sum, value) => sum + value, 0) / values.length,
            p50: percentile(sorted, 0.5),
            p95: percentile(sorted, 0.95),
            max: sorted.at(-1),
            sum: values.reduce((sum, value) => sum + value, 0),
          },
        ] as const;
      }),
    ),
  };
}

interface Model {
  id: string;
  is_downloaded: boolean;
}
interface Result {
  model: string;
  bound_backend: string;
  audio_secs: number;
  warm_mean_ms: number;
  transcribe_ms: number[];
  stream_chunk_ms: number | null;
  text: string;
  pipeline_runs: {
    timings: Record<string, number>;
    language?: string;
    text: string;
  }[];
}
const models: Model[] = JSON.parse(
  await run(["--list-models", "--json"], "models"),
);
const devices = await run(["--list-devices"], "devices");
const selected = models.filter(
  (m) =>
    m.is_downloaded &&
    (values.model
      ? values.model.includes(m.id)
      : /granite-speech-4\.1-2b.*Q4|Qwen3-ASR-(1\.7|0\.6)B|nemotron-3\.5.*Q[68]|parakeet-tdt-0\.6b-v3.*Q[48]|r2t2.*Q[48]/i.test(
          m.id,
        )),
);
if (!selected.length)
  throw new Error("No matching installed models; nothing was downloaded");
for (const id of values.model ?? []) {
  if (!selected.some((m) => m.id === id))
    throw new Error(`Model is not installed: ${id}`);
}
const baseline = values.baseline
  ? JSON.parse(readFileSync(values.baseline, "utf8"))
  : null;
const summary: { policy: string; cases: object[]; failures: object[] } = {
  policy: "3 runs: exclude warm-up run 1; arithmetic mean of runs 2 and 3",
  cases: [],
  failures: [],
};

interface DeviceTarget {
  index: string;
  kind: string;
  tag: string;
}

const parsedDevices: DeviceTarget[] = [];
for (const line of devices.split("\n")) {
  const m = line
    .trim()
    .match(/^index=(\d+)\s+kind=([^\s]+)\s+name=(.*?)\s+vram=/);
  if (m) {
    const idx = m[1];
    const kind = m[2].toLowerCase();
    const name = m[3].trim();
    const tag =
      kind === "vulkan"
        ? /nvidia/i.test(name)
          ? "vulkan_nvidia"
          : /intel/i.test(name)
            ? "vulkan_intel"
            : `vulkan_${idx}`
        : kind;
    parsedDevices.push({ index: idx, kind, tag });
  }
}

if (parsedDevices.length === 0) {
  for (const backend of ["cpu", "cuda", "vulkan"]) {
    const match = devices.match(new RegExp(`index=(\\d+) kind=${backend}\\b`));
    if (match) {
      parsedDevices.push({ index: match[1], kind: backend, tag: backend });
    }
  }
}

const targetDevices = parsedDevices.filter((d) => {
  if (!values.backend || values.backend.length === 0) return true;
  return values.backend.some(
    (b) => b.toLowerCase() === d.kind || b.toLowerCase() === d.tag,
  );
});

if (targetDevices.length === 0) {
  summary.failures.push({ error: "No matching compute devices available" });
}

for (const target of targetDevices) {
  for (const model of selected) {
    const supportsStreaming = /nemotron|r2t2/i.test(model.id);
    for (const streaming of supportsStreaming ? [false, true] : [false]) {
      const name = `${model.id.split("/").at(-1)}.${target.tag}.${streaming ? "stream" : "batch"}`;
      console.log(name);
      try {
        const args = [
          "--transcribe-file",
          wav,
          "--model",
          model.id,
          "--device-index",
          target.index,
          "--repeat",
          "3",
          "--json",
        ];
        if (streaming) {
          if (/r2t2/i.test(model.id)) {
            args.push("--stream-chunk-ms", "320");
          } else {
            args.push("--stream-chunk-ms", "16", "--stream-att-right", "6");
          }
        }
        const result: Result = JSON.parse(await run(args, name));
        if (!result.bound_backend.toLowerCase().startsWith(target.kind))
          throw new Error(
            `Requested ${target.kind}, loaded ${result.bound_backend}`,
          );
        if (result.transcribe_ms.length !== 3)
          throw new Error("Expected exactly 3 runs");
        if (
          result.pipeline_runs.length !== 3 ||
          result.pipeline_runs[1].text !== result.pipeline_runs[2].text
        )
          throw new Error("Measured runs produced different transcripts");
        const timings = Object.fromEntries(
          Object.keys(result.pipeline_runs[1].timings).map((key) => [
            key,
            (result.pipeline_runs[1].timings[key] +
              result.pipeline_runs[2].timings[key]) /
              2,
          ]),
        );
        const mean = (result.transcribe_ms[1] + result.transcribe_ms[2]) / 2;
        if (Math.abs(result.warm_mean_ms - mean) > 0.001)
          throw new Error("Score must be the arithmetic mean of runs 2 and 3");
        const row = {
          executable_sha256: executableSha256,
          nearby_native_files: runtimeFiles,
          case: name,
          ...result,
          wav_sha256: wavSha256,
          warm_mean_timings: timings,
          // Mirrors the native summary's field name so a single reporter can
          // render app and native summaries alike.
          backend: result.bound_backend,
          // Null for batch cases, and for a staged library without the chunk
          // instrumentation; the reporter leaves the columns blank then.
          stage_metrics: streaming
            ? stageMetrics(resolve(directory, `${name}.log`), 3)
            : null,
        };
        writeFileSync(
          resolve(directory, `${name}.json`),
          JSON.stringify(row, null, 2),
        );
        const old = baseline?.cases.find(
          (c: { case: string }) => c.case === name,
        );
        if (old) {
          if (
            old.bound_backend !== result.bound_backend ||
            old.audio_secs !== result.audio_secs ||
            (old.wav_sha256 && old.wav_sha256 !== wavSha256) ||
            old.pipeline_runs[1].language !==
              result.pipeline_runs[1].language ||
            old.stream_chunk_ms !== result.stream_chunk_ms
          ) {
            throw new Error("Baseline configuration mismatch");
          }
          if (
            result.warm_mean_ms >
            old.warm_mean_ms * (1 + Number(values["max-regression"]))
          ) {
            summary.failures.push({ case: name, error: "Timing regression" });
          }
          if (old.text !== result.text)
            summary.failures.push({
              case: name,
              error: "Transcript changed; review required",
            });
        }
        summary.cases.push(row);
      } catch (error) {
        summary.failures.push({ case: name, error: String(error) });
      }
      writeFileSync(
        resolve(directory, "summary.json"),
        JSON.stringify(summary, null, 2),
      );
    }
  }
}
writeFileSync(
  resolve(directory, "summary.json"),
  JSON.stringify(summary, null, 2),
);
if (summary.failures.length) process.exitCode = 1;
