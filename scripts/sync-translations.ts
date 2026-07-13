import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const LOCALES_DIR = path.join(__dirname, "..", "src", "i18n", "locales");
const REFERENCE_LANG = "en";

type TranslationData = Record<string, any>;

function getLanguages(): string[] {
  const entries = fs.readdirSync(LOCALES_DIR, { withFileTypes: true });
  return entries
    .filter((entry) => entry.isDirectory() && entry.name !== REFERENCE_LANG)
    .map((entry) => entry.name)
    .sort();
}

function deepSync(
  ref: TranslationData,
  target: TranslationData,
): TranslationData {
  const result: TranslationData = {};

  for (const key in ref) {
    if (!Object.prototype.hasOwnProperty.call(ref, key)) continue;

    const refVal = ref[key];
    const targetVal = target[key];

    if (
      typeof refVal === "object" &&
      refVal !== null &&
      !Array.isArray(refVal)
    ) {
      // It's a sub-object
      const targetSub =
        typeof targetVal === "object" &&
        targetVal !== null &&
        !Array.isArray(targetVal)
          ? targetVal
          : {};
      result[key] = deepSync(refVal, targetSub);
    } else {
      // It's a leaf value (string, array, boolean, number, etc.)
      if (targetVal !== undefined) {
        result[key] = targetVal;
      } else {
        result[key] = refVal; // Fallback to English value
      }
    }
  }

  return result;
}

function syncTranslations(): void {
  console.log("Syncing translation files...");

  const refPath = path.join(LOCALES_DIR, REFERENCE_LANG, "translation.json");
  if (!fs.existsSync(refPath)) {
    console.error(`Error: Reference translation file not found at ${refPath}`);
    process.exit(1);
  }

  const refContent = fs.readFileSync(refPath, "utf8");
  const refData = JSON.parse(refContent) as TranslationData;

  const languages = getLanguages();
  let syncCount = 0;

  for (const lang of languages) {
    const langFilePath = path.join(LOCALES_DIR, lang, "translation.json");
    let langData: TranslationData = {};

    if (fs.existsSync(langFilePath)) {
      try {
        const content = fs.readFileSync(langFilePath, "utf8");
        langData = JSON.parse(content);
      } catch (err: any) {
        console.warn(
          `Warning: Failed to parse JSON for ${lang}, overwriting: ${err.message}`,
        );
      }
    }

    const syncedData = deepSync(refData, langData);

    // Normalize line endings to CRLF for consistency on Windows/other platforms
    const formattedJson = JSON.stringify(syncedData, null, 2);
    const withCRLF = formattedJson
      .replace(/\r\n/g, "\n")
      .replace(/\n/g, "\r\n");

    fs.writeFileSync(langFilePath, withCRLF, "utf8");
    console.log(`  ✓ Synced ${lang}/translation.json`);
    syncCount++;
  }

  console.log(`Successfully synced ${syncCount} translation files.`);
}

syncTranslations();
