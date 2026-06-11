# Contributing Translations to S2B2S

Thank you for helping translate S2B2S! This guide explains how to add or improve translations.

---

## Quick Start

1. Fork the repository
2. Copy the English translation file to your language folder
3. Translate the values (not the keys!)
4. Submit a pull request

---

## File Structure

Translation files are located at `src/i18n/locales/`:

```
src/i18n/locales/
├── en/translation.json    # English (source language)
├── ar/translation.json    # Arabic
├── bg/translation.json    # Bulgarian
├── cs/translation.json    # Czech
├── de/translation.json    # German
├── es/translation.json    # Spanish
├── fr/translation.json    # French
├── he/translation.json    # Hebrew
├── it/translation.json    # Italian
├── ja/translation.json    # Japanese
├── ko/translation.json    # Korean
├── pl/translation.json    # Polish
├── pt/translation.json    # Portuguese
├── ru/translation.json    # Russian
├── sv/translation.json    # Swedish
├── tr/translation.json    # Turkish
├── uk/translation.json    # Ukrainian
├── vi/translation.json    # Vietnamese
├── zh/translation.json    # Chinese (Simplified)
└── zh-TW/translation.json # Chinese (Traditional)
```

---

## Adding a New Language

### Step 1: Create the Language Folder

Use the [ISO 639-1 language code](https://en.wikipedia.org/wiki/List_of_ISO_639-1_codes):

```bash
mkdir src/i18n/locales/[language-code]
```

### Step 2: Copy the English File

```bash
cp src/i18n/locales/en/translation.json src/i18n/locales/[language-code]/translation.json
```

### Step 3: Translate the Values

Translate only the **values** (right side), not the keys (left side):

```json
{
  "sidebar": {
    "general": "General",      // ← Translate this value
    "advanced": "Advanced",    // ← Translate this value
    ...
  }
}
```

### Step 4: Register Your Language

Edit `src/i18n/languages.ts` and add your language metadata:

```typescript
export const LANGUAGE_METADATA: Record<string, { name: string; nativeName: string }> = {
  en: { name: "English", nativeName: "English" },
  es: { name: "Spanish", nativeName: "Español" },
  fr: { name: "French", nativeName: "Français" },
  de: { name: "German", nativeName: "Deutsch" },
  // ← Add your language here
};
```

### Step 5: Test Your Translation

1. Run the app: `bun run tauri dev`
2. Go to Settings → General → App Language
3. Select your language and verify all text displays correctly

### Step 6: Submit a Pull Request

1. Commit your changes
2. Push to your fork
3. Open a pull request with the language name in the title

---

## Improving Existing Translations

Found a typo or better translation?
1. Edit the relevant `translation.json` file
2. Submit a PR with a brief description of the change
3. Run `bun run check:translations` to verify format

---

## Translation Guidelines

### Do:
- Use natural, native-sounding language
- Keep translations concise (UI space is limited)
- Match the tone of the English text (friendly, clear)
- Preserve technical terms when appropriate (e.g., "API", "GPU")
- Ensure RTL languages (Arabic, Hebrew) render correctly

### Don't:
- Translate brand names (S2B2S, Whisper, Parakeet, OpenAI, Piper, Kokoro)
- Change or remove `{{variables}}` (e.g., `{{error}}`, `{{model}}`)
- Modify JSON keys or structure
- Add extra spaces or formatting

### Handling Variables

```json
// English
"downloadModel": "Failed to download model: {{error}}"

// French (correct)
"downloadModel": "Échec du téléchargement du modèle : {{error}}"

// French (incorrect - don't translate the variable!)
"downloadModel": "Échec du téléchargement du modèle : {{erreur}}"
```

### Handling Plurals

Use a general form that works for all cases. Proper plural support may be added in the future.

---

## Currently Supported Languages

| Language | Code | Status |
|----------|------|--------|
| Arabic | `ar` | Complete |
| Bulgarian | `bg` | Complete |
| Czech | `cs` | Complete |
| German | `de` | Complete |
| English | `en` | Complete (source) |
| Spanish | `es` | Complete |
| French | `fr` | Complete |
| Hebrew | `he` | Complete |
| Italian | `it` | Complete |
| Japanese | `ja` | Complete |
| Korean | `ko` | Complete |
| Polish | `pl` | Complete |
| Portuguese | `pt` | Complete |
| Russian | `ru` | Complete |
| Swedish | `sv` | Complete |
| Turkish | `tr` | Complete |
| Ukrainian | `uk` | Complete |
| Vietnamese | `vi` | Complete |
| Chinese (Simplified) | `zh` | Complete |
| Chinese (Traditional) | `zh-TW` | Complete |

**Total: 20 languages**

---

## Requested Languages

We'd love help with additional languages! Check the [issues](https://github.com/NairoDorian/S2B2S/issues) for open translation requests.

---

## Verification

Run the translation check script to verify your changes:

```bash
bun run check:translations
```

This checks for missing keys, extra keys, and format issues across all locale files.

---

Thank you for making S2B2S accessible to more people around the world!
