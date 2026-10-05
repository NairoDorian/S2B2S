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
complete, `lint`, `typecheck`, the unit checks (`test:unit`), `format:check`,
and the repomix pack. The
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

The `--prerelease` flag is not optional **in this project**: it tracks the newest
published versions of its dependencies on purpose, so a release candidate is what
the next release is tested against. A bare `bun run update-deps` with no flag
still runs in stable mode, so type the flag yourself — `bun run update` always
appends it and is the command to run before starting a release. Add `--dry-run`
to either to see what would move without writing anything.

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
  (`src-tauri/Cargo.toml`). `nvcc` must be on `PATH` and must support your
  MSVC version. `bun run build:fast` compiles kernels for the local GPU only
  (`TRANSCRIBE_CUDA_ARCHITECTURES=auto`); `bun run build:full` builds the full
  architecture matrix. `bun run dev:cpu` needs no CUDA toolkit at all — it
  configures the native build with `-DTRANSCRIBE_CUDA=OFF` and runs on the CPU
  backend. Every lane compiles the full set of model architectures in.

- [Vulkan SDK](https://vulkan.lunarg.com/sdk/home) from LunarG. This is **not**
  optional: the same Cargo target also asks for the **`vulkan`** feature, so
  ggml-vulkan and its `vulkan-shaders-gen` host sub-build (which needs the SDK's
  headers and `glslc`) are part of every build, including `--cpu`:

  ```powershell
  winget install KhronosGroup.VulkanSDK
  ```

  Open a new terminal afterward so `VULKAN_SDK` is set. ggml-vulkan also does
  `find_package(SPIRV-Headers CONFIG REQUIRED)`; the SDK normally satisfies
  that, and when it does not, the header-only package via vcpkg does — see
  [SPIRV-Headers not found](#spirv-headers-not-found-on-windows-x64) in
  Troubleshooting.

> [!NOTE]
> **Windows on ARM is CPU-only.** `src-tauri/Cargo.toml` links transcribe.cpp
> there with no features at all, so there is no CUDA, no Vulkan and no need for
> either SDK on that target.

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
- OpenBLAS development libraries (`libopenblas-dev` / `openblas-devel` /
  `openblas`) — the deb and rpm bundles declare `libopenblas` as a runtime
  dependency
- X11 development libraries (`libx11-dev`, `libxtst-dev`, `libxrandr-dev` and
  `libxkbcommon-dev`) for enigo's keyboard-simulation backend
- The Vulkan toolchain (`vulkan-sdk` / `vulkan-headers` + `shaderc`, which
  provides `glslc`) and the SPIR-V headers. This fork builds transcribe.cpp with
  the **`cuda`** _and_ **`vulkan`** features on Linux (`src-tauri/Cargo.toml`),
  so both the CUDA Toolkit (`nvcc` on `PATH`) and the Vulkan SDK are required;
  release CI installs LunarG's `vulkan-sdk` from its own apt repository for
  exactly this reason. `bun run dev:cpu` drops the CUDA half but keeps Vulkan.
- Install with:

  ```bash
  # Ubuntu/Debian
  sudo apt update
  sudo apt install build-essential clang libclang-dev libevdev-dev libasound2-dev pkg-config libssl-dev libopenblas-dev \
    libx11-dev libxtst-dev libxrandr-dev libxkbcommon-dev \
    libvulkan-dev vulkan-tools glslc spirv-headers glslang-tools \
    libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
    libgtk-layer-shell0 libgtk-layer-shell-dev patchelf cmake

  # Fedora/RHEL
  sudo dnf groupinstall "Development Tools"
  sudo dnf install alsa-lib-devel openblas-devel pkgconf openssl-devel \
    libX11-devel libXtst-devel libXrandr-devel libxkbcommon-devel \
    vulkan-devel glslc spirv-headers-devel spirv-tools-devel glslang \
    clang clang-devel libevdev-devel \
    gtk3-devel webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel \
    gtk-layer-shell gtk-layer-shell-devel \
    cmake

  # Arch Linux
  sudo pacman -S base-devel clang libevdev shaderc spirv-headers glslang \
    alsa-lib openblas pkgconf openssl vulkan-devel \
    libx11 libxtst libxrandr xkbcommon \
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
bun run dev:cpu
```

`bun run tauri` goes through `scripts/tauri-runner.ts`, which first checks
whether the pinned `transcribe-cpp` commit is behind the
`NairoDorian/transcribe.cpp` fork's `main` and, if so, runs
`cargo update -p transcribe-cpp -p transcribe-cpp-sys`. It tracks the
`tauri-plugin-*` git pins the same way, against
`tauri-apps/plugins-workspace`'s `v3` branch. Neither check ever fails the
build, even offline. There is no VAD model file to fetch — the detector is
pure Rust — and speech models come from the in-app catalog on first run.

### Build lanes

Every lane packs the **full** model-architecture set; they differ on exactly one
axis, the CUDA architecture policy:

| Command                           | Native configure                            | Reach for it when                                                           |
| --------------------------------- | ------------------------------------------- | --------------------------------------------------------------------------- |
| `bun run dev:cpu` / `build:cpu`   | `-DTRANSCRIBE_CUDA=OFF`, private cache root | the usual working loop — no CUDA toolkit needed                             |
| `bun run dev:fast` / `build:fast` | `TRANSCRIBE_CUDA_ARCHITECTURES=auto`        | the change touches the GPU path (kernels, device selection, VRAM, a timing) |
| `bun run dev:full` / `build:full` | `TRANSCRIBE_CUDA_ARCHITECTURES=default`     | anything release-shaped                                                     |

A bare `bun run tauri dev` / `bun run tauri build` is the **fast** lane, not the
CPU one. The lanes are mutually exclusive (`build --fast --cpu` is refused rather
than silently resolved). Every lane still builds Vulkan — `--cpu` only turns CUDA
off — so the Vulkan SDK is required either way. Pruning of stale
`src-tauri/target` artifacts runs automatically before each one
(`scripts/prune-target.ts`); set `ZER0_NO_PRUNE=1` to skip it for a shell.

### 4. Build for Production

ZER0 provides three release build commands:

```bash
# Fast local build (auto-detects and compiles CUDA kernels only for your local GPU):
bun run build:fast

# Full distribution build (compiles complete multi-architecture CUDA matrix):
bun run build:full

# CPU-only smoke build (no CUDA at all):
bun run build:cpu
```

- **`build:fast` (`bun run build:fast` or `bun run tauri build --fast`)**: Ideal for local development release testing. Automatically sets `TRANSCRIBE_CUDA_ARCHITECTURES=auto` to target solely the active system GPU, dramatically cutting compile time.
- **`build:full` (`bun run build:full` or `bun run tauri build --full`)**: Used for releasing distribution packages. Compiles the full matrix of CUDA architectures to run across all supported NVIDIA GPU generations. A bare `bun run tauri build` is the fast local build, not this one.
- **`build:cpu` (`bun run build:cpu` or `bun run tauri build --cpu`)**: Turns CUDA off at configure time; the app runs on the CPU backend. A quick check that a release build still compiles and bundles.

This compiles a release binary and generates platform-specific bundles (deb, rpm, AppImage on Linux; dmg on macOS; NSIS installer and MSI on Windows). Windows binaries are not Authenticode-signed (upstream's `signCommand`, Azure Trusted Signing, was removed — see the troubleshooting note below). **Updater artifacts are signed**: `createUpdaterArtifacts` is on and every release bundle produces `.sig` files and an updater manifest with the project's own minisign key.

#### Updater artifact signing

The signing key lives at `~/.tauri/zer0.key` (generated by `bun x tauri signer generate`); the matching public key is `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`. The tauri runner exports it as `TAURI_SIGNING_PRIVATE_KEY` automatically when the environment does not provide one, so local `build:fast` / `build:full` sign without setup. CI reads the same key from the `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository secrets, gated on the workflows' `sign-binaries` input (both release workflows pass `true`). The private key is a release secret: never commit it, never lose it — updates stop verifying without it.

**The pair was rotated on 2026-09-22**, because the private key was not on the
build machine and no copy existed anywhere. Two consequences are worth knowing
before the next release, and one format trap is worth knowing before touching
the config:

- Installs built before the rotation pin the old public key, so they reject
  artifacts signed with the new one (`verify_signature` fails with
  `UnexpectedKeyId`), and they update once by re-downloading the installer by
  hand; after that they carry the new key and update normally. Nothing has been
  released publicly and the developer's own machine holds the only install, so
  this costs one reinstall and no user is affected.
- The `TAURI_SIGNING_PRIVATE_KEY` repository secret now holds the new key:
  `gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/zer0.key` (set
  2026-09-22; `gh secret list` showed an empty list before it, so there was no
  retired key to remove). `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` must be unset or
  empty, since this pair was generated with `--ci` and carries no password.

`plugins.updater.pubkey` is base64 **of the minisign public key text**, and the
consumer settles it rather than convention: `verify_signature` calls
`base64_to_string(pub_key)` before `PublicKey::decode`
(`plugins/updater/src/updater.rs:1537` of the pinned `plugins-workspace`
checkout). The alpha CLI writes exactly that value into `~/.tauri/zer0.key.pub`,
so the config value is that file's contents, trimmed — _not_ a second base64
encoding of them. Encoding the file's bytes again produces a 204-character
value that decodes to base64 instead of to `untrusted comment: minisign public
key: …`, and nothing catches it until an update fails to verify in the field.
Both shapes decode without error; only one verifies.

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

The raw binary (`src-tauri/target/release/zer0`) cannot run standalone — it needs Tauri resource files (tray icons, sounds) to be co-located at the expected path.

**Install from the deb bundle** (works on any Linux distro):

```bash
cd /tmp
ar x /path/to/ZER0/src-tauri/target/release/bundle/deb/ZER0_*_amd64.deb data.tar.gz  # `_arm64.deb` on aarch64
tar xzf data.tar.gz
sudo cp usr/bin/zer0 /usr/bin/
sudo cp -a usr/lib/. /usr/lib/
sudo cp -r usr/share/icons/hicolor/* /usr/share/icons/hicolor/
sudo cp usr/share/applications/ZER0.desktop /usr/share/applications/
```

The runtime libraries live in the app-private `/usr/lib/ZER0/` (on the binary's rpath), so no `ldconfig` step is needed.

After subsequent rebuilds, copy the binary and any refreshed runtime libraries:

```bash
sudo cp src-tauri/target/release/zer0 /usr/bin/
sudo mkdir -p /usr/lib/ZER0
sudo cp -a src-tauri/transcribe-libs/. /usr/lib/ZER0/
```

`src-tauri/build.rs` stages those libraries during the cargo build, and
`tauri.conf.json` bundles them into `/usr/lib/ZER0` for the deb and rpm and into
`/usr/lib` for the AppImage — so a bundled install needs neither step. CI's
AppImage path runs the same copy as a checked script,
`scripts/ci/stage-transcribe-libs.sh <install-lib-dir> <dest>`, which fails
loudly if `libtranscribe.so` or a `libggml-cpu*.so` backend module is missing.

Resources only need re-copying if they change upstream (new icons, sounds, models, etc.).

## Nix / NixOS

`flake.nix` packages ZER0 for Linux (`x86_64` and `aarch64`) and provides
modules for NixOS and home-manager. On Nix the native dependency set is declared
in the flake rather than installed from your distro, so nothing above is needed:

```bash
nix build .#zer0            # build the package (~25 min cold; CI caches to Cachix)
nix develop                 # dev shell: rustc, cargo, bun, cargo-tauri, cmake, Vulkan headers
nix eval .#packages.x86_64-linux.zer0.drvPath   # fast syntax/eval check
```

`.nix/bun.nix` holds the per-package Bun fetch hashes generated from `bun.lock`
by `bun2nix`. `bun install` regenerates it through the `postinstall` hook
(`scripts/check-nix-deps.ts`), and CI fails when it is out of sync — re-run
`bun scripts/check-nix-deps.ts` on a Nix machine if you ever edit `package.json`
without running `bun install`.

Two deliberate differences from a source build:

- **The updater is force-disabled.** The package sets `ZER0_DISABLE_UPDATER=1`
  in its wrapper: self-update cannot work against an immutable `/nix/store` path.
  Updater artifacts are not signed from Nix either — the flake patches
  `bundle.createUpdaterArtifacts` off, since the signing key is not available
  inside the sandbox.
- **Global hotkeys need `/dev/uinput`.** The NixOS module adds the udev rule that
  opens it to the `input` group; add yourself to that group as well. The
  home-manager module instead ships a systemd _user_ service that autostarts the
  app with `graphical-session.target`:

  ```nix
  inputs.zer0.url = "github:NairoDorian/S2B2S";

  # NixOS — system package + the udev rule
  nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
    modules = [ zer0.nixosModules.default { programs.zer0.enable = true; } ];
  };
  users.users.you.extraGroups = [ "input" ];

  # home-manager — a systemd user service that autostarts it
  imports = [ zer0.homeManagerModules.default ];
  services.zer0.enable = true;
  ```

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

### SPIRV-Headers not found on Windows x64

ggml-vulkan (built whenever the `vulkan` feature is on, which is every Windows
x86_64 build) does `find_package(SPIRV-Headers CONFIG REQUIRED)` for
`<spirv-headers/spirv.hpp>`. ggml's CMake appends `$VULKAN_SDK` to
`CMAKE_PREFIX_PATH` first, so a normal LunarG SDK install satisfies it — but some
installs (and the CI SDK install among them) do not expose a findable
`SPIRV-HeadersConfig.cmake`, and the configure fails. Provide the header-only
package through vcpkg and put its prefix on `CMAKE_PREFIX_PATH`, which the cmake
crate forwards to CMake:

```powershell
vcpkg install spirv-headers:x64-windows
$prefix = "$env:VCPKG_INSTALLATION_ROOT/installed/x64-windows" -replace '\\','/'
$env:CMAKE_PREFIX_PATH = if ($env:CMAKE_PREFIX_PATH) { "$prefix;$env:CMAKE_PREFIX_PATH" } else { $prefix }
bun run tauri dev
```

It is a CONFIG package and header-only, so no vcpkg toolchain file is needed.
x86_64 only — the Windows ARM64 build is CPU-only and never builds ggml-vulkan.

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
verifying, so keep the private key backed up outside the machine — and if it is
already lost, the rotation procedure, its effect on existing installs, and the
exact shape `plugins.updater.pubkey` expects are in
[Updater artifact signing](#updater-artifact-signing) above. Verify a rotation
before publishing: build a bundle, then check the produced `.sig` against the
config's public key, since a mismatched pair builds and bundles cleanly and
only fails later, inside a user's app.
