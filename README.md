# ZER0

**A free, open source, and extensible speech-to-text application that works completely offline.**

ZER0 is a cross-platform desktop application that provides simple, privacy-focused speech transcription. Press a shortcut, speak, and have your words appear in any text field. This happens on your own computer without sending any information to the cloud.

It began as a personal fork of [Handy](https://github.com/cjpais/Handy) by CJ Pais — the
MIT-licensed foundation this project is built on, credited in [License](#license) — and
has since grown into its own project with its own name, its own identity and its own
release line.

## Why ZER0?

ZER0 was created to fill the gap for a truly open source, extensible speech-to-text tool.

- **Free**: Accessibility tooling belongs in everyone's hands, not behind a paywall
- **Open Source**: Together we can build further. Extend ZER0 for yourself and contribute to something bigger
- **Private**: Your voice stays on your computer. Get transcriptions without sending audio to the cloud
- **Simple**: One tool, one job. Transcribe what you say and put it into a text box

ZER0 isn't trying to be the best speech-to-text app—it's trying to be the most forkable one.


## How It Works

1. **Press** a configurable keyboard shortcut: hold it to record and release to stop, or tap it to toggle recording on and off (Hold-only and Toggle-only modes are also available)
2. **Speak** your words while the shortcut is active
3. **Release** and ZER0 transcribes your speech with the model you selected — no cloud, no API keys
4. **Get** your transcribed text pasted directly into whatever app you're using

The process is entirely local:

- Silence is filtered using VAD (Voice Activity Detection) with Earshot, a pure-Rust detector with threshold hysteresis (sensitivity adjustable in Settings → Advanced)
- Transcription uses your choice of models from the bundled catalog:
  - **GGUF models only**, all through transcribe.cpp with GPU acceleration when available: Whisper family (Small/Medium/Turbo/Large), **Parakeet**, Moonshine, SenseVoice, Canary, Cohere, Voxtral, Qwen3-ASR, …
- Works on Windows, macOS, and Linux

### Multi-STT Mode (Fork Feature)

This fork adds **Multi-STT** — run up to four speech-to-text models simultaneously and merge their outputs for higher accuracy. Each extra model runs on its own inference engine in parallel, and results are combined either by concatenation or via an LLM merge prompt.

**Key capabilities:**

- **Parallel transcription**: Primary + three extra models all transcribe the same audio concurrently
- **Per-model language selection**: Each extra model can use a different recognition language
- **Per-model translation**: Each extra model can optionally translate to English
- **Parallel model loading**: Extra models are pre-loaded in parallel during the recording phase
- **LLM merge prompt**: Optionally merge multiple transcriptions through an OpenAI-compatible LLM (including local llama.cpp servers) using `${output}`, `${output2}`, `${output3}`, and `${output4}` placeholders
- **Keep models loaded**: Retain extra models in memory between uses for faster repeat transcriptions (on by default; only matters when the model unload timeout is "Immediately")
- **Manual model unload**: Free model memory on demand via the settings UI
- **Dedicated shortcut**: Configurable `multi_stt_transcribe` binding separate from the standard transcription shortcut
- **Performance mode** (optional): Simulate a "full power" keyboard shortcut while Multi-STT is decoding and a "normal" shortcut afterwards, so an external power-profile tool can boost the CPU only when needed

**To enable:** Open Settings → Multi-STT, toggle on, select your second, third, and fourth models, set the Multi-STT hotkey in Settings → General, and optionally configure a merge prompt using the same post-processing LLM provider.

> **Note:** this fork ships **without** default transcription hotkeys (both `transcribe` and `multi_stt_transcribe` are empty on a fresh install, so the performance-mode simulated keys can never retrigger ZER0). Set them once in Settings → General → Shortcuts.

### Other Fork Additions

- **Transcribe Files**: batch-transcribe audio files or whole folders to `.txt` / `.md`, cut into segments at quiet points, with plain, post-processed, Multi-STT and Multi-STT + post-processing modes
- **Live Mode**: keep the microphone open indefinitely — session audio is saved as chunked WAV files and the live transcription is mirrored into a text file while you speak
- **Live FFT**: a real-time spectrum analyser for the microphone (log / mel / ERB / bark scales, EQ, windows, A/C/468 weighting, ballistics, waterfall), plus a miniature analyser built into the recording overlay
- **Local LLM (llama.cpp)**: download and supervise a local llama.cpp server from within the app for post-processing and Multi-STT merging — no API key needed
- **Help page and shortcut cheat sheet**: every page links its settings to a searchable Help anchor, and a keyboard button in the window corner shows all assigned shortcuts
- **Noise suppression (RNNoise)**: optional pure-Rust RNNoise on the microphone before voice detection and transcription, with a live VAD test next to the threshold slider to hear-and-see the difference
- **transcribe.cpp only, pure-Rust VAD**: no ONNX Runtime, no `transcribe-rs`, no VAD model file. Voice activity detection is **Earshot** with threshold hysteresis (upstream's Silero wrapper spoke the v4 tensor interface against a v6 model and silently passed every frame through)
- **Recordings with no speech are never decoded** (under 200 ms of measured speech) — no more hallucinated text pasted from silence
- **Speech stats in the overlay**: speaking/paused indicator, a timer that only runs while you talk, and live words-per-minute with streaming models
- **Direct streaming paste**: type the live transcript character by character into the target app as it is committed, with a speed control. Plain transcription only — with post-processing or Multi-STT the live stream is shown in the Live overlay as a preview and the processed result is pasted once with Ctrl+V
- **Raw uncompressed audio saving**: keep recordings at the captured sample rate and format (32-bit float, 24-bit or 16-bit PCM) before resampling and VAD filtering, with no latency impact
- **Windows real-time low latency**: high priority process class, EcoQoS opt-out, 1 ms multimedia timer resolution, MMCSS capture thread scheduling, and hardware buffer size minimization
- **CUDA GPU backend** on Windows x86_64 and Linux (via the `NairoDorian/transcribe.cpp` fork) instead of Vulkan; `bun run build:fast` compiles kernels for your GPU only
- **Status-bar model controls**: switch models, pick a quantization (with an in-place benchmark against your latest recording), and choose a native streaming latency preset
- **History tools**: delete all recordings, vacuum the database, open the models folder
- **Accent colour palette**, configurable microphone idle timeout, append-trailing-newline option
- **Headless CLI**: `zer0 --transcribe-file recording.wav` (see below)

## Quick Start

### Installation

1. Download the latest release from the [releases page](https://github.com/NairoDorian/S2B2S/releases)
2. Install the application
3. Launch ZER0 and grant necessary system permissions (microphone, accessibility)
4. Configure your preferred keyboard shortcuts in Settings
5. Start transcribing!

> **Note:** there is no Homebrew cask or winget package for ZER0, and no
> distribution channel we do not publish ourselves. Releases come from this
> repository's release page only.

### Development Setup

For detailed build instructions including platform-specific requirements, see [BUILD.md](BUILD.md).

## Architecture

ZER0 is built as a Tauri application combining:

- **Frontend**: Solid 2 + TypeScript with Tailwind CSS for the settings UI (migrated from React on 2026-09-13)
- **Backend**: Rust for system integration, audio processing, and ML inference
- **Core Libraries**:
  - `transcribe-cpp`: Local speech recognition for every model (GGML/GGUF: Whisper family, Parakeet, Moonshine, Canary, …)
  - `cpal`: Cross-platform audio I/O
  - `earshot`: pure-Rust voice activity detection
  - `nnnoiseless`: pure-Rust RNNoise noise suppression (optional)
  - `rdev`: Global keyboard shortcuts and system events
  - `rubato`: Audio resampling

## Sponsors (Upstream)

<div align="center">
  We're grateful for the support of the upstream project's sponsors:
  <br><br>
  <a href="https://wordcab.com">
    <img src="sponsor-images/wordcab.png" alt="Wordcab" width="120" height="120">
  </a>
  &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;
  <a href="https://github.com/epicenter-so/epicenter">
    <img src="sponsor-images/epicenter.png" alt="Epicenter" width="120" height="120">
  </a>
  &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;
  <a href="https://boltai.com?utm_source=handy">
    <img src="sponsor-images/boltai.jpg" alt="Bolt AI" width="120" height="120">
  </a>
  &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;
  <a href="https://cantydigital.com.au/">
    <img src="sponsor-images/cantydigital.png" alt="Canty Digital" width="120" height="120">
  </a>
</div>

### Debug Mode

ZER0 includes an advanced debug mode for development and troubleshooting. Access it by pressing:

- **macOS**: `Cmd+Shift+D`
- **Windows/Linux**: `Ctrl+Shift+D`

### CLI Parameters

ZER0 supports command-line flags for controlling a running instance and customizing startup behavior. These work on all platforms (macOS, Windows, Linux).

**Remote control flags** (sent to an already-running instance via the single-instance plugin):

```bash
zer0 --toggle-transcription    # Toggle recording on/off
zer0 --toggle-post-process     # Toggle recording with post-processing on/off
zer0 --cancel                  # Cancel the current operation
```

**Startup flags:**

```bash
zer0 --start-hidden            # Start without showing the main window
zer0 --no-tray                 # Start without the system tray icon
zer0 --debug                   # Enable debug mode with verbose logging
zer0 --help                    # Show all available flags
```

**Headless transcription** (runs the batch path without a microphone and exits; the model must already be installed):

```bash
zer0 --transcribe-file recording.wav          # mono WAV: 16/24-bit PCM or 32-bit float, any sample rate
zer0 -f recording.wav --model <model-id>      # pick a model instead of the selected one
zer0 -f recording.wav --device-index 0        # GPU device for GGUF models
zer0 -f recording.wav --repeat 3 --json       # timing runs, machine-readable output
zer0 --list-models                            # installed model ids
zer0 --list-devices                           # GPU devices
```

Flags can be combined for autostart scenarios:

```bash
zer0 --start-hidden --no-tray
```

> **macOS tip:** When ZER0 is installed as an app bundle, invoke the binary directly:
>
> ```bash
> /Applications/ZER0.app/Contents/MacOS/zer0 --toggle-transcription
> ```

## Known Issues & Current Limitations

This project is actively being developed and has some [known issues](https://github.com/NairoDorian/S2B2S/issues). We believe in transparency about the current state:

### Bluetooth Headset Microphones (macOS)

Using a Bluetooth headset microphone on macOS may temporarily reduce playback quality or volume while recording because Bluetooth switches to bidirectional audio. Keep your headphones as the output device and select your Mac's built-in or an external microphone in ZER0 to avoid this.

### fn and Globe Key Shortcuts (macOS)

Shortcuts that include the `fn` (Globe) key **only work on Apple keyboards** — your Mac's built-in keyboard or an Apple external keyboard. They will never trigger on a third-party keyboard, even while it is connected to the same Mac.

This is a hardware limitation rather than a ZER0 bug. `fn` is not part of the standard USB HID keyboard specification: Apple reports it through a vendor-specific usage that macOS honors only from Apple devices, while third-party keyboards handle their `Fn` key entirely in firmware and send nothing to the computer. There is no event for ZER0 to listen for.

If you switch between a MacBook keyboard and an external one, pick a shortcut built from standard modifiers (`ctrl`, `option`, `shift`, `command`) or a regular key instead.

### Linux Notes

**Text Input Tools:**

For reliable text input on Linux, install the appropriate tool for your display server:

| Display Server | Recommended Tool | Install Command                                    |
| -------------- | ---------------- | -------------------------------------------------- |
| X11            | `xdotool`        | `sudo apt install xdotool`                         |
| Wayland        | `wtype`          | `sudo apt install wtype`                           |
| Both           | `dotool`         | `sudo apt install dotool` (requires `input` group) |

- **X11**: Install `xdotool` for both direct typing and clipboard paste shortcuts
- **Ubuntu 26.04**: Has Wayland display server by default. `wtype` does not work, you need to install `ydotool` and configure systemd as described [in the upstream project](https://github.com/cjpais/Handy/pull/557#issuecomment-3781249267).
- **Wayland**: Install `wtype` (preferred) or `dotool` for text input to work correctly
- **dotool setup**: Requires adding your user to the `input` group: `sudo usermod -aG input $USER` (then log out and back in)

Without these tools, ZER0 falls back to enigo which may have limited compatibility, especially on Wayland.

**Wayland Support (Linux):**

- Limited support for Wayland display server
- Requires [`wtype`](https://github.com/atx/wtype) or [`dotool`](https://sr.ht/~geb/dotool/) for text input to work correctly (see [Linux Notes](#linux-notes) below for installation)

**Other Notes:**

- **Runtime library dependency (`libgtk-layer-shell.so.0`)**:
  - ZER0 links `gtk-layer-shell` on Linux. If startup fails with `error while loading shared libraries: libgtk-layer-shell.so.0`, install the runtime package for your distro:

    | Distro        | Package to install    | Example command                        |
    | ------------- | --------------------- | -------------------------------------- |
    | Ubuntu/Debian | `libgtk-layer-shell0` | `sudo apt install libgtk-layer-shell0` |
    | Fedora/RHEL   | `gtk-layer-shell`     | `sudo dnf install gtk-layer-shell`     |
    | Arch Linux    | `gtk-layer-shell`     | `sudo pacman -S gtk-layer-shell`       |

  - For building from source on Ubuntu/Debian, you may also need `libgtk-layer-shell-dev`.

- The recording overlay is disabled by default on Linux (`Overlay Position: None`) because certain compositors treat it as the active window. When the overlay is visible it can steal focus, which prevents ZER0 from pasting back into the application that triggered transcription. If you enable the overlay anyway, be aware that clipboard-based pasting might fail or end up in the wrong window.
- If you are having trouble with the app, running with the environment variable `WEBKIT_DISABLE_DMABUF_RENDERER=1` may help
- If ZER0 fails to start reliably on Linux, see [Troubleshooting → Linux Startup Crashes or Instability](#linux-startup-crashes-or-instability).
- **Global keyboard shortcuts (Wayland):** On Wayland, system-level shortcuts must be configured through your desktop environment or window manager. Use the [CLI flags](#cli-parameters) as the command for your custom shortcut.

  **GNOME:**
  1. Open **Settings > Keyboard > Keyboard Shortcuts > Custom Shortcuts**
  2. Click the **+** button to add a new shortcut
  3. Set the **Name** to `Toggle ZER0 Transcription`
  4. Set the **Command** to `zer0 --toggle-transcription`
  5. Click **Set Shortcut** and press your desired key combination (e.g., `Super+O`)

  **KDE Plasma:**
  1. Open **System Settings > Shortcuts > Custom Shortcuts**
  2. Click **Edit > New > Global Shortcut > Command/URL**
  3. Name it `Toggle ZER0 Transcription`
  4. In the **Trigger** tab, set your desired key combination
  5. In the **Action** tab, set the command to `zer0 --toggle-transcription`

  **Sway / i3:**

  Add to your config file (`~/.config/sway/config` or `~/.config/i3/config`):

  ```ini
  bindsym $mod+o exec zer0 --toggle-transcription
  ```

  **Hyprland:**

  Add to your config file (`~/.config/hypr/hyprland.conf`):

  ```ini
  bind = $mainMod, O, exec, zer0 --toggle-transcription
  ```

- You can also trigger ZER0 externally via Unix signals or the CLI flags, which lets Wayland window managers or other hotkey daemons keep ownership of keybindings:

  | Action                                    | Trigger                                                |
  | ----------------------------------------- | ------------------------------------------------------ |
  | Toggle transcription                      | `pkill -USR2 -n zer0` or `zer0 --toggle-transcription` |
  | Toggle transcription with post-processing | `zer0 --toggle-post-process`                           |

  Example Sway config:

  ```ini
  bindsym $mod+o exec pkill -USR2 -n zer0
  bindsym $mod+p exec zer0 --toggle-post-process
  ```

  `pkill` here simply delivers the signal—it does not terminate the process.

  > **Behavior change:** older releases also accepted `SIGUSR1` for toggling transcription with post-processing. WebKitGTK — the webview engine embedded in ZER0 on Linux — uses SIGUSR1 internally to coordinate JavaScript garbage collection, so listening for it caused phantom recordings and interrupted dictations every few minutes (upstream issue #1660). ZER0 no longer listens for SIGUSR1 on Linux; the post-processing toggle is still available via `zer0 --toggle-post-process`. **Remove any `pkill -USR1` bindings**: the signal is now delivered straight to WebKit's internal handler and can crash the app.

**Overlay & Pasting Issues (Linux):**

- The recording overlay window can interfere with pasting transcribed text into target applications on Linux (X11)
- **Solution:** Open **Settings > Advanced** and set **"Overlay Position"** to **"None"** to disable the overlay
- Enable **"Audio Feedback"** (also in Advanced) if you still want audible confirmation of recording state
- Users who upgrade from older versions or import settings from other platforms may need to manually apply this change

### Platform Support

- **macOS (both Intel and Apple Silicon)**
- **x64 Windows**
- **x64 Linux**

### System Requirements/Recommendations

The following are recommendations for running ZER0 on your own machine. If you don't meet the system requirements, the performance of the application may be degraded. We are working on improving the performance across all kinds of computers and hardware.

**For Whisper Models:**

- **macOS**: M series Mac, Intel Mac
- **Windows**: Intel, AMD, or NVIDIA GPU
- **Linux**: Intel, AMD, or NVIDIA GPU
  - Ubuntu 22.04, 24.04

**For Parakeet / Moonshine / Canary (GGUF) models:**

- Same requirements as Whisper: they run through transcribe.cpp on the GPU when one is available, otherwise on the CPU
- Parakeet V3 auto-detects the language across 25 European languages

## Roadmap & Active Development

We're actively working on several features and improvements. Contributions and feedback are welcome! Engineering work that is open right now is tracked in [docs/KNOWN_ISSUES.md](docs/KNOWN_ISSUES.md).

### In Progress

**macOS Keyboard Improvements:**

- Support for Globe key as transcription trigger
- A rewrite of global shortcut handling for macOS, and potentially other OS's too.

**Settings Refactoring:**

- Cleanup and refactor settings system which is becoming bloated and messy
- Implement better abstractions for settings management

Debug logging (file logs with a configurable level — see [docs/LOGGING.md](docs/LOGGING.md)) and typed Tauri command bindings (tauri-specta) have shipped; the ideas parked there are done.

## Release Integrity

ZER0's updater artifacts are **signed**. `createUpdaterArtifacts` is on, and
every release bundle is signed with the project's own minisign key whose
public half is `plugins.updater.pubkey` in
[`src-tauri/tauri.conf.json`](src-tauri/tauri.conf.json). The in-app updater
(`check()` → `downloadAndInstall()`) verifies that signature before
installing, so an update is only applied when it was signed by this key.

Release builds are not Authenticode-signed (no `signCommand`; see
[BUILD.md](BUILD.md) for the signing setup, including where the private key
lives and the `TAURI_SIGNING_PRIVATE_KEY` repository secret release CI
reads).

## Troubleshooting

### Previous Clipboard Content Is Pasted Instead of the Transcription

If the transcription is correct in **History** but ZER0 inserts text you copied earlier, see upstream issue #502. With the standard clipboard paste method, ZER0 restores your previous clipboard after a fixed delay. Under load, the receiving application may read the clipboard only after that restoration.

1. Open ZER0's settings window and press `Cmd+Shift+D` (macOS) or `Ctrl+Shift+D` (Windows/Linux) to reveal **Debug**.
2. On **macOS and Windows**, try **Reliable Paste (Beta)** in Debug with a clipboard paste method selected. It uses clipboard read notifications to delay restoration instead of relying on the standard fixed delay. Test it in the application where the problem occurs; it is still experimental.
3. If Reliable Paste is disabled or unavailable, increase **Paste Delay (After)** in Debug and test again. This controls the wait before restoring your previous clipboard. **Paste Delay (Before)** controls the wait before sending the paste keystroke and addresses a different part of the operation. These delay settings apply to the standard paste path, not Reliable Paste.

If the problem persists, add your ZER0 version, operating system, receiving application, paste method, Reliable Paste setting, and before/after delays to the existing issue. Redact private dictated text before sharing logs.

### Manual Model Installation (For Proxy Users or Network Restrictions)

If you're behind a proxy, firewall, or in a restricted network environment where ZER0 cannot download models automatically, you can manually download and install them. The URLs are publicly accessible from any browser.

#### Step 1: Find Your App Data Directory

1. Open ZER0 settings
2. Navigate to the **About** section
3. Copy the "App Data Directory" path shown there, or use the shortcuts:
   - **macOS**: `Cmd+Shift+D` to open debug menu
   - **Windows/Linux**: `Ctrl+Shift+D` to open debug menu

The typical paths are:

- **macOS**: `~/Library/Application Support/com.nairodorian.zer0/`
- **Windows**: `C:\Users\{username}\AppData\Roaming\com.nairodorian.zer0\`
- **Linux**: `~/.config/com.nairodorian.zer0/`

#### Step 2: Create Models Directory

Inside your app data directory, create a `models` folder if it doesn't already exist:

```bash
# macOS/Linux
mkdir -p ~/Library/Application\ Support/com.nairodorian.zer0/models

# Windows (PowerShell)
New-Item -ItemType Directory -Force -Path "$env:APPDATA\com.nairodorian.zer0\models"
```

#### Step 3: Download Model Files

Download the models you want from below

**Whisper Models (single .bin files):**

- Small (487 MB): `https://blob.handy.computer/ggml-small.bin`
- Medium (492 MB): `https://blob.handy.computer/whisper-medium-q4_1.bin`
- Turbo (1600 MB): `https://blob.handy.computer/ggml-large-v3-turbo.bin`
- Large (1100 MB): `https://blob.handy.computer/ggml-large-v3-q5_0.bin`

**Parakeet Unified EN 0.6B (single `.gguf` file, recommended):**

- Q8_0 (731 MB): `https://huggingface.co/handy-computer/parakeet-unified-en-0.6b-gguf/resolve/main/parakeet-unified-en-0.6b-Q8_0.gguf`

**Parakeet TDT 0.6B v3 (single `.gguf` file, multilingual):**

- Q8_0: `https://huggingface.co/handy-computer/parakeet-tdt-0.6b-v3-gguf/resolve/main/parakeet-tdt-0.6b-v3-Q8_0.gguf`


#### Step 4: Install Models

**For Whisper Models (.bin files):**

Simply place the `.bin` file directly into the `models` directory:

```
{app_data_dir}/models/
├── ggml-small.bin
├── whisper-medium-q4_1.bin
├── ggml-large-v3-turbo.bin
└── ggml-large-v3-q5_0.bin
```

**For GGUF Models (.gguf files):**

Place the `.gguf` file directly into the `models` directory, exactly like the Whisper `.bin` files above. ZER0 also picks up models already present in the shared Hugging Face cache (`~/.cache/huggingface/hub`), so a copy downloaded by another tool works without being moved.

**Important Notes:**

- Do not rename the `.bin` or `.gguf` files—use the exact filenames from the download URLs
- After placing the files, restart ZER0 to detect the new models

#### Step 5: Verify Installation

1. Restart ZER0
2. Open Settings → Models
3. Your manually installed models should now appear as "Downloaded"
4. Select the model you want to use and test transcription

### Custom Whisper Models

ZER0 can auto-discover custom Whisper GGML models placed in the `models` directory. This is useful for users who want to use fine-tuned or community models not included in the default model list.

**How to use:**

1. Obtain a Whisper model in GGML `.bin` format (e.g., from [Hugging Face](https://huggingface.co/models?search=whisper%20ggml))
2. Place the `.bin` file in your `models` directory (see paths above)
3. Restart ZER0 to discover the new model
4. The model will appear in the "Custom Models" section of the Models settings page

**Important:**

- Community models are user-provided and may not receive troubleshooting assistance
- The model must be a valid Whisper GGML format (`.bin` file)
- Model name is derived from the filename (e.g., `my-custom-model.bin` → "My Custom Model")

### Linux Startup Crashes or Instability

If ZER0 fails to start reliably on Linux — for example, it crashes shortly after launch, never shows its window, or reports a Wayland protocol error — try the steps below in order.

**1. Install (or reinstall) `gtk-layer-shell`**

ZER0 uses `gtk-layer-shell` for its recording overlay and links against it at runtime. A missing or broken installation is the most common cause of startup failures and can manifest as a crash or a hang well before any window is shown. Make sure the runtime package is installed for your distro:

| Distro        | Package to install    | Example command                        |
| ------------- | --------------------- | -------------------------------------- |
| Ubuntu/Debian | `libgtk-layer-shell0` | `sudo apt install libgtk-layer-shell0` |
| Fedora/RHEL   | `gtk-layer-shell`     | `sudo dnf install gtk-layer-shell`     |
| Arch Linux    | `gtk-layer-shell`     | `sudo pacman -S gtk-layer-shell`       |

If it is already installed and you still see startup problems, try reinstalling it (e.g. `sudo pacman -S gtk-layer-shell` again) in case the library files were corrupted by a partial upgrade.

**2. Disable the GTK layer shell overlay (`ZER0_NO_GTK_LAYER_SHELL`)**

If installing the library does not help, you can skip `gtk-layer-shell` initialization entirely as a workaround. On some compositors (notably KDE Plasma under Wayland) it has been reported to interact poorly with the recording overlay. With this variable set, the overlay falls back to a regular always-on-top window:

```bash
ZER0_NO_GTK_LAYER_SHELL=1 zer0
```

**3. Disable WebKit DMA-BUF renderer (`WEBKIT_DISABLE_DMABUF_RENDERER`)**

On some GPU/driver combinations the WebKitGTK DMA-BUF renderer can cause the window to fail to render or to crash. Try:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 zer0
```

**Making a workaround permanent**

Once you've found a flag that helps, export it from your shell profile (`~/.bashrc`, `~/.zshenv`, …) or from the desktop autostart entry that launches the app. If you launch it from a `.desktop` file, you can prefix the `Exec=` line, e.g.:

```ini
Exec=env ZER0_NO_GTK_LAYER_SHELL=1 zer0
```

If a workaround helps you, please [open an issue](https://github.com/NairoDorian/S2B2S/issues) describing your distro, desktop environment, and session type — that information helps us narrow down the underlying bug.

### Vulkan Overlays and Capture Tools on Windows (`ZER0_KEEP_VULKAN_IMPLICIT_LAYERS`)

On Windows, the app asks the Vulkan loader to skip implicit layers to avoid crashes caused by overlay and capture hooks (upstream issue upstream issue #2049). GPU acceleration remains enabled; this does not change system-wide settings.

To opt out for GPU selection or debugging tools, fully quit the app (including the tray icon), then run both commands in the same PowerShell window:

```powershell
$env:ZER0_KEEP_VULKAN_IMPLICIT_LAYERS = "1"
& "$env:ProgramFiles\ZER0\zer0.exe"
```

Adjust the executable path if needed. This override only applies to apps launched from that PowerShell session, not the Start menu. The app also preserves any existing `VK_LOADER_LAYERS_DISABLE` value.

This fork runs inference through CUDA and never loads the Vulkan loader, so the setting changes nothing about acceleration here; it is kept so the process environment matches upstream.

### ZER0 Starts or Stops Recording on Its Own (Linux)

Handy 0.9.4 and earlier listened for `SIGUSR1` as a remote-control trigger. WebKitGTK — the webview engine embedded in Handy on Linux — uses that same signal internally to coordinate JavaScript garbage collection, so GC cycles were misread as hotkey presses: recordings started on their own, or real dictations were cut off mid-sentence (typically ~2 minutes in). See upstream issue #1660.

Update to a newer release, and replace any `pkill -USR1 -n zer0` keybindings with `zer0 --toggle-post-process`.


### How to Contribute

1. **Check existing issues** at [github.com/NairoDorian/S2B2S/issues](https://github.com/NairoDorian/S2B2S/issues)
2. **Fork the repository** and create a feature branch
3. **Test thoroughly** on your target platform — and run `bun run precommit`
4. **Submit a pull request** with clear description of changes
5. **Read [CONTRIBUTING.md](CONTRIBUTING.md)** for the toolchain and the pre-commit routine every change is expected to pass

The goal is to create both a useful tool and a foundation for others to build upon—a well-patterned, simple codebase that serves the community.



## License

MIT License - see [LICENSE](LICENSE) file for details.

ZER0 is open-source software, but the ZER0 name, logo, icon, and brand assets are not open-source. Unofficial forks, rewrites, and redistributions must use their own branding and must not imply endorsement or affiliation.

The upstream **[Handy](https://github.com/cjpais/Handy)** project by CJ Pais is MIT
licensed, and its copyright notice is preserved in [LICENSE](LICENSE) as the licence
requires. All of ZER0's own branding, naming and assets are original to this project.
The Handy name, logo, icon and brand assets belong to their authors and are also not
open-source: its mention here is attribution, not affiliation.

## Acknowledgments

- **Handy** by CJ Pais — the MIT-licensed speech-to-text application this project was forked from
- **Whisper** by OpenAI for the speech recognition model
- **ggml and transcribe.cpp** for amazing cross-platform speech-to-text inference/acceleration
- **Earshot** for a fast, pure-Rust VAD
- **RNNoise** (Xiph) and **nnnoiseless** for noise suppression
- **Tauri** team for the excellent Rust-based app framework
- **Community contributors** helping make ZER0 better
