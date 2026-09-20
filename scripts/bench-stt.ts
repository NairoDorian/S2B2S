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
      : /granite-speech-4\.1-2b.*Q4|Qwen3-ASR-(1\.7|0\.6)B|nemotron-3\.5.*Q[68]|parakeet-tdt-0\.6b-v3.*Q[48]/i.test(
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
for (const backend of ["cpu", "cuda"]) {
  const match = devices.match(new RegExp(`index=(\\d+) kind=${backend}\\b`));
  if (!match) {
    summary.failures.push({ backend, error: "Device unavailable" });
    continue;
  }
  for (const model of selected) {
    for (const streaming of /nemotron/i.test(model.id)
      ? [false, true]
      : [false]) {
      const name = `${model.id.split("/").at(-1)}.${backend}.${streaming ? "stream" : "batch"}`;
      console.log(name);
      try {
        const args = [
          "--transcribe-file",
          wav,
          "--model",
          model.id,
          "--device-index",
          match[1],
          "--repeat",
          "3",
          "--json",
        ];
        if (streaming)
          args.push("--stream-chunk-ms", "16", "--stream-att-right", "6");
        const result: Result = JSON.parse(await run(args, name));
        if (!result.bound_backend.toLowerCase().startsWith(backend))
          throw new Error(
            `Requested ${backend}, loaded ${result.bound_backend}`,
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
