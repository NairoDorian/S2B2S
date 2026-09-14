// Standalone assert check, run by `bun test` (bunfig.toml roots discovery at
// src/). Originally run as a bare script before the runner existed.
import { test } from "bun:test";
import assert from "node:assert";
import { APP_NAME, RELEASES_URL } from "@/lib/appIdentity";
import { resolvePortableInstallerUrl } from "./portableInstaller";

// The fixtures are built from the generated identity rather than typed out, so a
// rename or a version bump cannot leave a test asserting against an address the
// app no longer uses. `v<version>` is only a sample tag — the resolver never
// parses it.
const TAG_URL = `${RELEASES_URL.replace(/\/latest$/, "")}/download/v0.9.7`;
const X64_SETUP = `${TAG_URL}/${APP_NAME}_0.9.7_x64-setup.exe`;
const ARM64_SETUP = `${TAG_URL}/${APP_NAME}_0.9.7_arm64-setup.exe`;

// Trimmed copy of the real latest.json served from the updater endpoint.
const manifest = {
  version: "0.9.7",
  platforms: {
    "windows-x86_64-nsis": { url: X64_SETUP, signature: "…" },
    "windows-x86_64-msi": {
      url: `${TAG_URL}/${APP_NAME}_0.9.7_x64_en-US.msi`,
    },
    "windows-aarch64-nsis": { url: ARM64_SETUP, signature: "…" },
    "darwin-aarch64": {
      url: `${TAG_URL}/${APP_NAME}_aarch64.app.tar.gz`,
    },
  },
};

test("portableInstaller resolves the right installer for every shape of latest.json", () => {
  // x64 Windows -> the x64 NSIS asset, pinned to the release tag
  assert.equal(
    resolvePortableInstallerUrl(manifest, "windows", "x86_64"),
    X64_SETUP,
  );

  // arm64 Windows -> the arm64 NSIS asset
  assert.equal(
    resolvePortableInstallerUrl(manifest, "windows", "aarch64"),
    ARM64_SETUP,
  );

  // no manifest (check() failed or returned no update) -> releases page fallback
  assert.equal(
    resolvePortableInstallerUrl(undefined, "windows", "x86_64"),
    RELEASES_URL,
  );

  // no NSIS bundle for this arch -> releases page fallback
  assert.equal(
    resolvePortableInstallerUrl(manifest, "windows", "x86"),
    RELEASES_URL,
  );

  // non-Windows portable install -> releases page, never a Windows .exe
  assert.equal(
    resolvePortableInstallerUrl(manifest, "macos", "aarch64"),
    RELEASES_URL,
  );

  // malformed manifest -> releases page fallback
  assert.equal(
    resolvePortableInstallerUrl({ platforms: "nope" }, "windows", "x86_64"),
    RELEASES_URL,
  );
});
