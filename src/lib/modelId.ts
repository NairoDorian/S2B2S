/**
 * Turning a raw model id into something a user should read.
 *
 * Model ids are the string the backend and the model files on disk are keyed
 * by — `handy-computer/whisper-large-v3-turbo-gguf/whisper-large-v3-turbo-Q8_0`
 * — and every one of those parts is noise in a list of recordings: the org is
 * always the same, the file extension carries no information, and the quant
 * suffix is the one piece worth seeing.
 *
 * The org is **not ours**. It is the Hugging Face organisation that actually
 * hosts the catalog files, so it is matched by name here rather than being
 * derived from `appIdentity.ts`: this is an external address, and the app
 * renaming does not move the files. `scripts/check-identity.ts` carries the
 * matching exemption, because a rename sweep must not "fix" it.
 */

/** The upstream Hugging Face org every catalog model id starts with. */
const MODEL_ORG_PREFIX = /^handy-computer\//;

/** The GGUF file extension a catalog id ends with. */
const GGUF_SUFFIX = /\.gguf$/;

/**
 * The id as it should appear in a list: no org, no extension.
 *
 * `whisper-large-v3-turbo-gguf/whisper-large-v3-turbo-Q8_0` rather than
 * `handy-computer/whisper-large-v3-turbo-gguf/whisper-large-v3-turbo-Q8_0.gguf`
 * — only the org prefix and the extension are dropped; the repo name stays.
 * An id that matches neither pattern — a custom model, or a value from an older
 * release — is returned unchanged, so this can never lose information.
 */
export function displayModelId(modelId: string): string {
  return modelId.replace(MODEL_ORG_PREFIX, "").replace(GGUF_SUFFIX, "");
}

/**
 * Look up a per-model setting across quant variants and base repo IDs.
 *
 * If `modelId` has a direct entry in `map`, returns it.
 * If `modelId` is a quant variant (`repo/filename.gguf`), checks the base repo
 * and sibling quants. If `modelId` is a base repo without `.gguf`, checks if
 * any quant variant of that repo has an entry.
 */
export function resolveModelSetting<T>(
  map: { [key: string]: T } | undefined | null,
  modelId: string | null | undefined,
  fallback: T,
): T {
  if (!map || !modelId) return fallback;
  if (map[modelId] !== undefined) return map[modelId];
  if (modelId.endsWith(".gguf")) {
    const lastSlash = modelId.lastIndexOf("/");
    if (lastSlash > 0) {
      const repo = modelId.substring(0, lastSlash);
      if (map[repo] !== undefined) return map[repo];
      const sibling = Object.keys(map).find(
        (k) => k.startsWith(`${repo}/`) && map[k] !== undefined,
      );
      if (sibling && map[sibling] !== undefined) return map[sibling];
    }
  } else {
    const variant = Object.keys(map).find(
      (k) => k.startsWith(`${modelId}/`) && map[k] !== undefined,
    );
    if (variant && map[variant] !== undefined) return map[variant];
  }
  return fallback;
}
