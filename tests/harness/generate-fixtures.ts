/**
 * Phase 0(b): regenerate `fixtures.generated.ts` from `src/bindings.ts`.
 *
 * Run with:  bun tests/harness/generate-fixtures.ts
 *
 * Why generate rather than hand-write: the payloads the settings window needs
 * to boot are ~100 fields of `AppSettings` plus a dozen structured command
 * results, and their only source of truth is the Rust structs, which the
 * browser test cannot call. A hand-written fixture would drift silently every
 * time a field is added, and the failure would look like an unrelated crash in
 * the net — a `null` where a page wanted an array reads as `Cannot read
 * properties of null (reading 'length')`, which points at the page, not the
 * fixture. Reading the generated bindings instead means a new field surfaces
 * here as a diff: the fixture is wrong loudly, at generation time, instead of
 * quietly, in a test that fails somewhere else.
 *
 * The values are type-appropriate placeholders, NOT the real defaults. That is
 * deliberate: these fixtures are only required to make the shell boot and
 * paint, so they must be *well-typed*, not *accurate*. Tests must not assert on
 * a value that came from here.
 */

import { readFileSync, writeFileSync } from "fs";

const BINDINGS = "src/bindings.ts";
const OUT = "tests/harness/fixtures.generated.ts";

const src = readFileSync(BINDINGS, "utf8");

/** Body of a top-level `export type X = ...;` (balanced braces). */
function typeBody(name: string): string | null {
  const start = src.indexOf(`export type ${name} =`);
  if (start < 0) return null;
  const open = src.indexOf("{", start);
  if (open < 0) return null;
  let depth = 0;
  for (let i = open; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}") {
      depth--;
      if (depth === 0) return src.slice(open + 1, i);
    }
  }
  return null;
}

/** A union of string literals, e.g. `export type Theme = "light" | "dark";` */
function literalUnion(name: string): string[] | null {
  const start = src.indexOf(`export type ${name} =`);
  if (start < 0) return null;
  const end = src.indexOf(";", start);
  if (end < 0) return null;
  const rhs = src.slice(src.indexOf("=", start) + 1, end).trim();
  if (!rhs.includes('"')) return null;
  const lits = [...rhs.matchAll(/"([^"]*)"/g)].map((m) => m[1]);
  return lits.length ? lits : null;
}

/** Field lines of an object type body, comments stripped. */
function fields(body: string): { key: string; type: string }[] {
  const stripped = body
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\/\/.*$/gm, "");
  const out: { key: string; type: string }[] = [];
  for (const line of stripped.split("\n")) {
    const m = line.match(/^\s*(\w+)\??\s*:\s*(.+?),?\s*$/);
    if (m) out.push({ key: m[1], type: m[2].trim().replace(/,$/, "") });
  }
  return out;
}

/**
 * Values the generated placeholder is wrong for, as literal source.
 *
 * These are not cosmetic. `onboarding_completed` in particular decides which
 * tree mounts: with it false the app renders `AccessibilityOnboarding`, whose
 * `checkInitial` effect depends on a non-memoized callback and re-runs forever
 * — an infinite render loop that hangs the page before any assertion can run.
 * The net exists to guard the *main window*, which is also the state a
 * returning user is in, so that is what the fixture boots into.
 */
const OVERRIDES: Record<string, string> = {
  onboarding_completed: "true",
  // Rust-only migration marker; the frontend never reads it. High so no
  // frontend path can mistake the fixture for an old store.
  settings_schema_version: "999",
  app_language: '"en"',
};

/**
 * Command name → the binding type its handler must return, for every command
 * whose result a page reads structurally.
 *
 * The split against `tauri-mock.js`'s literals is deliberate and is about what
 * a wrong value *does*, not about tidiness:
 *
 *   - Here: the result is an object or an array the page reads a field off.
 *     `null` is not a value it can handle — it is a crash that unmounts the
 *     whole window. The shape must track `src/bindings.ts` mechanically.
 *   - There: a scalar, or a type `bindings.ts` declares as `| null`, where
 *     `null` is a legal answer ("no llama install", "no stats yet"). A literal
 *     reads better at the call site than an indirection through this table.
 *
 * The second group is the one to be careful about: `bindings.ts` does not wrap
 * these in `typedError` (they are raw `__TAURI_INVOKE<T>` calls), which is
 * exactly what makes them unforgiving — the value goes straight to the page.
 */
const IPC_PAYLOADS: Record<string, string> = {
  // Settings-ish queries.
  get_extra_loaded_models: "string[]",
  get_available_typing_tools: "string[]",
  get_available_accelerators: "AvailableAccelerators",
  get_secure_input_status: "SecureInputStatus",
  get_windows_microphone_permission_status: "WindowsMicrophonePermissionStatus",
  check_custom_sounds: "CustomSounds",

  // History and statistics.
  get_history_entries: "PaginatedHistory",
  get_statistics_summary: "StatisticsSummary",

  // Models.
  get_arch_plugins: "ArchPluginInfo[]",

  // Local LLM.
  get_llama_server_state: "LlamaServerStateEvent",
  get_llama_server_logs: "string[]",
  list_gguf_files: "GgufFile[]",
  list_installed_llama_servers: "InstalledLlamaServer[]",
  fetch_llama_releases: "LlamaRelease[]",
  get_llama_command_preview: "string",

  // The two job pages, whose status commands are raw `__TAURI_INVOKE<Status>`
  // and whose components read `.phase` off the result immediately.
  get_file_transcription_status: "FileTranscriptionStatus",
  live_mode_status: "LiveModeStatus",
  live_mode_list_sessions: "LiveSessionInfo[]",
  live_fft_status: "LiveFftStatus",
  live_fft_raw_defaults: "LiveFftSettings",
};

const unresolved = new Set<string>();

/**
 * A placeholder value as source. `indent` is the whitespace the value's own
 * closing brace sits at, so a nested object comes out already prettier-clean
 * and the generated file does not have to be formatted afterwards.
 */
function resolve(type: string, depth = 0, indent = ""): string {
  if (depth > 6) return "null";
  const t = type.trim().replace(/,$/, "");

  if (t === "boolean") return "false";
  if (t === "number") return "0";
  if (t === "string") return '""';
  if (t === "null" || t === "undefined") return "null";
  if (t === "unknown" || t === "any") return "null";

  // Homogeneous maps and arrays.
  if (/^\{.*\[key in string\].*\}$/.test(t) || /^Record<.*>$/.test(t))
    return "{}";
  if (/\[\]$/.test(t)) return "[]";
  if (/^Array</.test(t)) return "[]";

  const objectLiteral = (fs: { key: string; type: string }[]) => {
    const pad = indent + "  ";
    return (
      "{\n" +
      fs
        .map((f) => `${pad}${f.key}: ${resolve(f.type, depth + 1, pad)},`)
        .join("\n") +
      `\n${indent}}`
    );
  };

  // Nested object literal.
  if (t.startsWith("{")) {
    const inner = t.slice(1, t.lastIndexOf("}"));
    const fs = fields(inner);
    return fs.length ? objectLiteral(fs) : "{}";
  }

  // Union: prefer the first non-null branch.
  const branches = t.split("|").map((b) => b.trim());
  if (branches.length > 1) {
    const nonNull = branches.filter((b) => b !== "null" && b !== "undefined");
    if (!nonNull.length) return "null";
    return resolve(nonNull[0], depth + 1, indent);
  }

  // Named type: a literal union, or another object type.
  const lits = literalUnion(t);
  if (lits) return JSON.stringify(lits[0]);

  const body = typeBody(t);
  if (body !== null) {
    const fs = fields(body);
    if (!fs.length) {
      unresolved.add(t);
      return "{}";
    }
    return objectLiteral(fs);
  }

  unresolved.add(t);
  return "null";
}

/** Resolve a binding type, erroring if it is not a type this file can see. */
function requireType(type: string, why: string): string {
  const value = resolve(type, 0, "  ");
  if (value === "null" || unresolved.has(type)) {
    console.error(
      `${why} names type "${type}", which ${BINDINGS} does not define.`,
    );
    process.exit(1);
  }
  return value;
}

// ---- settings ------------------------------------------------------------

const body = typeBody("AppSettings_Serialize");
if (!body) {
  console.error("Could not find AppSettings_Serialize in " + BINDINGS);
  process.exit(1);
}

const settingsFields = fields(body);

for (const key of Object.keys(OVERRIDES)) {
  if (!settingsFields.some((f) => f.key === key)) {
    console.error(
      `OVERRIDES names "${key}", which is no longer a settings field.`,
    );
    process.exit(1);
  }
}

const settingsEntries = settingsFields
  .map((f) => `  ${f.key}: ${OVERRIDES[f.key] ?? resolve(f.type, 0, "  ")},`)
  .join("\n");

// ---- structured IPC payloads --------------------------------------------

const ipcEntries = Object.entries(IPC_PAYLOADS)
  .map(
    ([cmd, type]) =>
      `  ${cmd}: ${requireType(type, `IPC_PAYLOADS["${cmd}"]`)},`,
  )
  .join("\n");

const out = `// GENERATED by tests/harness/generate-fixtures.ts — do not edit by hand.
// Re-run after \`bun run tauri dev\` regenerates src/bindings.ts.
//
// Well-typed placeholders, not the app's real defaults: these exist only so the
// settings window boots in a browser. Never assert on a value that comes from here.
export const settingsFixture = {
${settingsEntries}
};

/** Command name → the structured result \`tauri-mock.js\` must return for it. */
export const ipcFixtures: Record<string, unknown> = {
${ipcEntries}
};

export default { settingsFixture, ipcFixtures };
`;

writeFileSync(OUT, out);
console.log(
  `wrote ${OUT}: ${settingsFields.length} settings fields, ${Object.keys(IPC_PAYLOADS).length} IPC payloads`,
);
if (unresolved.size) {
  console.log(`\nunresolved types (emitted as {} or null — check these):`);
  [...unresolved].sort().forEach((u) => console.log("  " + u));
}
