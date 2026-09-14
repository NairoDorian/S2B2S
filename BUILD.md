# Build Instructions

This guide covers how to set up the development environment and build ZER0 from source across different platforms.

> **NOTE:** ZER0 began as a fork of the MIT-licensed
> [Handy](https://github.com/cjpais/Handy) project by CJ Pais and is now its own
> project. The `Handy_Multi_STT` branch it was developed on keeps that name; the
> product, the binary and every path below are ZER0's own.

## Pre-commit routine

Every change goes through the same gate, and it is one command:

```bash
bun run hooks:install   # once per clone: points git at .githooks/
bun run precommit       # the gate, ~10 s
bun run precommit:full  # the gate plus clippy and the Rust test suite
```

`bun run precommit` runs, in order: identity mirrors (`meta:sync`), identity in
sync (`meta:check`), no stale product name (`check:identity`), translations
complete, `lint`, `typecheck`, `format:check`, and the repomix pack. The
`--full` variant adds `lint:backend` (clippy) and `test:backend` (the Rust test
suite), which is what `pre-commit` itself deliberately omits — see the header of
`scripts/pre-commit.ts` for why.

**Keeping things current is part of the routine, not a separate chore:**

```bash
bun run update          # rtk to latest, deps with --prerelease, then repomix
bun run update:rtk      # just the RTK CLI used by the maintainer's agent hook
bun run update-deps -- --prerelease
bun run docs:fetch      # refresh the mirrored stack docs (docs/STACK_WATCH.md)
```

`bun run update-deps` **always** takes `--prerelease` here: this project tracks
the newest published versions of its dependencies on purpose, so a release
candidate is what the next release is tested against. `bun run update` wraps
all three in the right order and is the command to run before starting a
release.

All shell commands in this project go through **`rtk`** (the token-optimizing
CLI proxy) with the single exception of `bun`, which is never proxied: `rtk bun
install` and friends are not supported, so `bun` is run directly and everything
else (`git`, `cargo`, `gh`, `node`, …) goes through `rtk`. Run `rtk gain` to
see what it has saved.

## Prerequisites

### All Platforms

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager
- [Tauri Prerequisites](https://tauri.app/start/prerequisites/)

### Platform-Specific Requirements

#### macOS

- Xcode Command Line Tools
- Install with: `xcode-select --install`

##### Intel Mac (x86_64)

No extra steps. This fork has no ONNX Runtime dependency, so Intel Macs build
exactly like Apple Silicon (transcribe.cpp with Metal).

#### Windows

- Microsoft C++ Build Tools: Visual Studio 2019/2022 with C++ development
  tools, or Visual Studio Build Tools 2019/2022
- [CMake](https://cmake.org/download/) (must be on `PATH`):

  ```powershell
  winget install Kitware.CMake
  ```

- [CUDA Toolkit](https://developer.nvidia.com/cuda-downloads) 13.x — this fork
  builds transcribe.cpp with the **`cuda`** feature on Windows x86_64
  (`src-tauri/Cargo.toml`), not upstream's Vulkan backend. `nvcc` must be on
  `PATH` and must support your MSVC version; `.cargo/config.toml` passes the
  flags CUDA 13.3 needs with MSVC 2026 (`-std=c++17 -Xcompiler=/Zc:preprocessor`)
  and turns sccache off for the native build. `bun run build:fast` compiles
  kernels for the local GPU only (`TRANSCRIBE_CUDA_ARCHITECTURES=auto`);
  `bun run build:full` builds the full architecture matrix.

- [Vulkan SDK](https://vulkan.lunarg.com/sdk/home) from LunarG — **only** if
  you switch the feature back to `vulkan` (`vulkan-shaders-gen` needs the
  SDK's headers and `glslc`):

  ```powershell
  winget install KhronosGroup.VulkanSDK
  ```

  Open a new terminal afterward so `VULKAN_SDK` is set.

> [!NOTE]
> Windows' 260-character path limit used to break the native Vulkan build in
> most checkouts. Since `transcribe-cpp` 0.1.3 the build works around it
> automatically (it compiles through a short NTFS junction — no admin rights
> or setup needed), so a normal checkout just builds. If you still hit
> path-limit errors, see
> [Windows build fails with path-limit errors](#windows-build-fails-with-path-limit-errors-msb3491--ftk1011--msb6003)
> in Troubleshooting.

#### Linux

- Build essentials
- ALSA development libraries
- This fork builds transcribe.cpp with the **`cuda`** feature on Linux (see
  `src-tauri/Cargo.toml`), so the [CUDA Toolkit](https://developer.nvidia.com/cuda-downloads)
  (`nvcc` on `PATH`) is required as well. The Vulkan packages below are only
  needed if you switch the feature back to `vulkan`.
- Install with:

  ```bash
  # Ubuntu/Debian
  sudo apt update
  sudo apt install build-essential clang libclang-dev libevdev-dev libasound2-dev pkg-config libssl-dev libvulkan-dev vulkan-tools glslc spirv-headers glslang-tools libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libgtk-layer-shell0 libgtk-layer-shell-dev patchelf cmake

  # Fedora/RHEL
  sudo dnf groupinstall "Development Tools"
  sudo dnf install alsa-lib-devel pkgconf openssl-devel vulkan-devel glslc \
    clang clang-devel libevdev-devel \
    spirv-headers-devel spirv-tools-devel glslang \
    gtk3-devel webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel \
    gtk-layer-shell gtk-layer-shell-devel \
    cmake

  # Arch Linux
  sudo pacman -S base-devel clang libevdev shaderc spirv-headers glslang alsa-lib pkgconf openssl vulkan-devel \
    gtk3 webkit2gtk-4.1 libappindicator-gtk3 librsvg gtk-layer-shell \
    cmake
  ```

## Setup Instructions

### 1. Clone the Repository

```bash
git clone git@github.com:NairoDorian/S2B2S.git
cd S2B2S
```

### 2. Install Dependencies

```bash
bun install
```

### 3. Start Dev Server

```bash
bun run tauri dev
```

`bun run tauri` goes through `scripts/tauri-runner.ts`, which first checks
whether the pinned `transcribe-cpp` commit is behind the
`NairoDorian/transcribe.cpp` fork's `main` and, if so, runs
`cargo update -p transcribe-cpp -p transcribe-cpp-sys` (it never fails the
build, even offline). There is no VAD model file to fetch — the detector is
pure Rust — and speech models come from the in-app catalog on first run.

### 4. Build for Production

ZER0 provides two release build commands:

```bash
# Fast local build (auto-detects and compiles CUDA kernels only for your local GPU):
bun run build:fast

# Full distribution build (compiles complete multi-architecture CUDA matrix):
bun run build:full
```

- **`build:fast` (`bun run build:fast` or `bun run tauri build --fast`)**: Ideal for local development release testing. Automatically sets `TRANSCRIBE_CUDA_ARCHITECTURES=auto` to target solely the active system GPU, dramatically cutting compile time.
- **`build:full` (`bun run build:full` or `bun run tauri build`)**: Used for releasing distribution packages. Compiles the full matrix of CUDA architectures to run across all supported NVIDIA GPU generations.

This compiles a release binary and generates platform-specific bundles (deb, rpm, AppImage on Linux; dmg on macOS; NSIS installer and MSI on Windows). Windows binaries are not Authenticode-signed (upstream's `signCommand`, Azure Trusted Signing, was removed — see the troubleshooting note below). **Updater artifacts are signed**: `createUpdaterArtifacts` is on and every release bundle produces `.sig` files and an updater manifest with the project's own minisign key.

The signing key lives at `~/.tauri/zer0.key` (generated by `bun x tauri signer generate`); the matching public key is `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`. The tauri runner exports it as `TAURI_SIGNING_PRIVATE_KEY` automatically when the environment does not provide one, so local `build:fast` / `build:full` sign without setup. CI reads the same key from the `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository secrets, gated on the workflows' `sign-binaries` input (both release workflows pass `true`). The private key is a release secret: never commit it, never lose it — updates stop verifying without it.

### Build folder size

Cargo never deletes the outputs of units that stopped existing, so every
dependency bump, transcribe.cpp pin bump or feature change leaves the
previous hashed artifacts, build-script outputs (a full native build each)
and incremental caches behind; `src-tauri/target` grows by gigabytes a week.
`bun run tauri`, `build:fast` and `build:full` therefore prune it before
every run (`scripts/prune-target.ts`): units whose version or git revision
is no longer in `Cargo.lock`, superseded incremental caches, all but the two
newest builds of the app crate, and installers of another version. Nothing
the next build needs is removed. To see what would go, or to run it by hand:

```powershell
bun run prune:target --dry-run   # list only
bun run prune:target --verbose   # remove and print every path
$env:ZER0_NO_PRUNE = "1"          # skip the automatic run for this shell
```

## Linux Install (from source)

The raw binary (`src-tauri/target/release/zer0`) cannot run standalone — it needs Tauri resource files (tray icons, sounds, VAD model) to be co-located at the expected path.

**Install from the deb bundle** (works on any Linux distro):

```bash
cd /tmp
ar x /path/to/ZER0/src-tauri/target/release/bundle/deb/ZER0_*_amd64.deb data.tar.gz
tar xzf data.tar.gz
sudo cp usr/bin/zer0 /usr/bin/
sudo cp -a usr/lib/. /usr/lib/
sudo cp -r usr/share/icons/hicolor/* /usr/share/icons/hicolor/
sudo cp usr/share/applications/ZER0.desktop /usr/share/applications/
```

The runtime libraries live in the app-private `/usr/usr/lib/ZER0/` (on the binary's rpath), so no `ldconfig` step is needed.

After subsequent rebuilds, copy the binary and any refreshed runtime libraries:

```bash
sudo cp src-tauri/target/release/zer0 /usr/bin/
sudo mkdir -p /usr/usr/lib/ZER0
sudo cp -a src-tauri/transcribe-libs/. /usr/usr/lib/ZER0/
```

Resources only need re-copying if they change upstream (new icons, sounds, models, etc.).

## Troubleshooting

### macOS Accessibility remains enabled after a local rebuild

Local builds use the ad-hoc `signingIdentity: "-"`. A rebuild can have a new macOS code
identity while the old **System Settings > Privacy & Security > Accessibility** entry
remains visibly enabled, leaving ZER0 on `Waiting...`.

After installing the final bundle at `/Applications/ZER0.app`, quit ZER0, clear only its
stale Accessibility record, then reopen it:

```bash
osascript -e 'tell application id "com.nairodorian.zer0" to quit' || true
tccutil reset Accessibility com.nairodorian.zer0
open /Applications/ZER0.app
```

Grant Accessibility again when prompted. This does not reset Microphone or other TCC
services, and official releases normally do not need it.

For optional diagnosis, compare the designated requirements of the previous and rebuilt
bundles:

```bash
codesign -dr - /path/to/previous/ZER0.app 2>&1
codesign -dr - /Applications/ZER0.app 2>&1
```

An ad-hoc requirement contains a `cdhash`; a changed requirement confirms the rebuild is
not covered by the old grant. The reset procedure does not require this check.

See upstream issue #1618 for the related onboarding
and stale-permission report.

### AppImage build fails on Arch / rolling-release distros

`linuxdeploy` bundles its own `strip` binary which is too old to process system libraries built with newer toolchains on rolling-release distros (Arch, CachyOS, Manjaro, EndeavourOS).

The error from Tauri:

```
Bundling ZER0_*_amd64.AppImage
failed to bundle project `failed to run linuxdeploy`
```

Tauri swallows the real linuxdeploy error. To see it, run linuxdeploy manually:

```bash
cd src-tauri/target/release/bundle/appimage
~/.cache/tauri/linuxdeploy-x86_64.AppImage --appimage-extract-and-run \
  --appdir ZER0.AppDir --plugin gtk --output appimage
```

**Workaround:** The binary, deb, and rpm bundles all build fine — only the AppImage step fails. To skip it:

```bash
bun run tauri build -- --bundles deb
```

Then install using the deb extraction method above.

### Windows build fails with path-limit errors (`MSB3491` / `FTK1011` / `MSB6003`)

On Windows the native build can fail partway through `transcribe-cpp-sys` with
any of these (all the same root cause):

```
error MSB3491: Could not write lines to file "...VCTargetsPath.tlog\VCTargetsPath.lastbuildstate".
Path: ... exceeds the OS max path limit. The fully qualified file name must be less than 260 characters.
```

```
FileTracker : error FTK1011: could not create the new file tracking log file:
...\vulkan-shaders-gen-build\...\cmTC_xxxxx.tlog\link.write.1.tlog.
The system cannot find the path specified.
```

```
error MSB6003: The specified task executable "CL.exe" could not be run.
System.IO.DirectoryNotFoundException: Could not find a part of the path ...
```

This is **not** a code or toolchain problem — it's Windows' legacy 260-character
path limit (`MAX_PATH`), overflowed by the Vulkan shader generator's nested
CMake build tree on top of Cargo's already-deep
`target\release\build\<crate>-<hash>\out\build\...` directory.

Since `transcribe-cpp` 0.1.3 this is mitigated automatically: the native build
compiles through a short NTFS junction under `%LOCALAPPDATA%\tcs` (created
without admin rights), so a normal checkout builds with no setup. Enabling
Windows long paths does **not** reliably help here — MSBuild's native
`FileTracker` (`tracker.exe`) ignores the long-paths flag — which is why the
junction, not the registry flag, is the fix.

If you still see the errors above, junction creation was likely blocked
(filesystem or corporate policy) — the failing build's log then contains a
`transcribe-cpp-sys: could not create short build junction ...` warning — or
your checkout is deep enough to overflow even the shortened layout. Work
around either case with a short Cargo target directory:

```powershell
# Per-shell:
$env:CARGO_TARGET_DIR = "C:\h"

# Or persist it for all future terminals (note: redirects ALL your
# Rust projects' build output, not just ZER0):
[Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR', 'C:\h', 'User')
```

Artifacts then land in `C:\h\release\...` instead of the repo's
`src-tauri\target\`. Open a **new terminal** if you persisted the variable —
it is only picked up by freshly started processes. Then `bun run tauri dev`
and `bun run tauri build` work normally.

### Windows `tauri build` fails at bundling with `program not found`

If the build compiles all the way to `Built application at: ...\zer0.exe` and
then fails with:

```
Signing C:\...\zer0.exe with a custom signing command
failed to bundle project `program not found`
```

that's the code-signing step. Upstream's `tauri.conf.json` configures a custom
`signCommand` (`trusted-signing-cli`, Azure Trusted Signing) that only exists
in the release CI environment; **this fork removed `signCommand`**, so the
error should not occur here unless you re-add it. Local development doesn't
need signing either way:

```powershell
# Development (no bundling/signing at all):
bun run tauri dev

# Or compile a release binary without the installer/signing step:
bun run tauri build --no-bundle
```

### Windows `tauri build` fails after bundling with `no private key`

If both installers are already listed under `Finished 2 bundles at:` and the
command still exits with

```
A public key has been found, but no private key. Make sure to set `TAURI_SIGNING_PRIVATE_KEY` environment variable.
```

that is the Tauri **updater** signing step. It runs on every bundle build
(`bundle.createUpdaterArtifacts` is on) when `TAURI_SIGNING_PRIVATE_KEY` is
not set. The tauri runner picks up the key from `~/.tauri/zer0.key`
automatically; if the error still appears, the file is missing or unreadable
— regenerate a pair with `bun x tauri signer generate -w ~/.tauri/zer0.key`,
put the _public_ key in `plugins.updater.pubkey`
(`src-tauri/tauri.conf.json`), and add the _private_ key as the
`TAURI_SIGNING_PRIVATE_KEY` repository secret (plus
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` when the key has one) so release builds
sign. A key pair you lose cannot be reconstructed: updates would stop
verifying, so keep the private key backed up outside the machine.
