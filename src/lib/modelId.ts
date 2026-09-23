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
