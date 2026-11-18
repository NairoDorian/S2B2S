import { describe, expect, it } from "bun:test";
import {
  pickerLanguage,
  suggestedSpokenLanguages,
} from "./languageSuggestions";

describe("pickerLanguage", () => {
  it("keeps the primary subtag in the picker's spelling", () => {
    expect(pickerLanguage("fr-FR")).toBe("fr");
    expect(pickerLanguage("zh-Hant-TW")).toBe("zh");
    expect(pickerLanguage("yue-Hant-HK")).toBe("yue");
    expect(pickerLanguage("nb-NO")).toBe("no");
    expect(pickerLanguage("fil-PH")).toBe("tl");
    expect(pickerLanguage("iw")).toBe("he");
    expect(pickerLanguage("jv-ID")).toBe("jw");
    expect(pickerLanguage("pt_BR")).toBe("pt");
  });
});

describe("suggestedSpokenLanguages", () => {
  it("offers the OS languages in order, deduplicated, with English", () => {
    // Upstream's cases, in the picker's spelling.
    expect(suggestedSpokenLanguages(["en-US", "zh-Hant-US"])).toEqual([
      "en",
      "zh",
    ]);
    expect(suggestedSpokenLanguages(["zh-Hans-CN", "en-GB"])).toEqual([
      "zh",
      "en",
    ]);
    expect(suggestedSpokenLanguages(["yue-Hant-HK"])).toEqual(["yue", "en"]);
    expect(suggestedSpokenLanguages(["en-US", "en-GB"])).toEqual(["en"]);
    expect(
      suggestedSpokenLanguages(["de-DE", "nb-NO", "no", "fr-FR", "es-ES"]),
    ).toEqual(["de", "no", "fr", "en"]);
  });

  it("skips what the picker does not offer, and always has English", () => {
    expect(suggestedSpokenLanguages(["xx-YY"])).toEqual(["en"]);
    expect(suggestedSpokenLanguages([])).toEqual(["en"]);
  });
});
