# Development Commands

## Environment Setup

```bash
bun install                              # Install dependencies
mkdir -p src-tauri/resources/models
curl -o src-tauri/resources/models/silero_vad_v4.onnx https://blob.handy.computer/silero_vad_v4.onnx
```

## Development

```bash
bun run tauri dev                         # Full app development (Rust + Vite)
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev  # macOS cmake fix
bun run dev                               # Frontend only (Vite dev server)
bun run build                             # Build frontend (TypeScript + Vite)
bun run tauri build                       # Production build (platform bundles)
bun run preview                           # Preview built frontend
```

## Linting & Formatting

```bash
bun run lint              # ESLint (frontend)
bun run lint:fix          # ESLint auto-fix
bun run format            # Prettier + cargo fmt
bun run format:check      # Check formatting only
bun run format:frontend   # Prettier only
bun run format:backend    # cargo fmt only
```

## Type Check & Build

```bash
bunx tsc --noEmit               # TypeScript type checking
cargo test export_bindings      # Regenerate src/bindings.ts (headless)
```

## Testing

```bash
bun run test:playwright         # Playwright E2E tests
bun run test:playwright:ui      # Playwright with UI
cargo test                      # Rust unit tests
bun run check:translations      # Verify i18n files
```

---

# Code Style Guidelines

## Rust (Backend)

- **Error handling**: Use `anyhow::Error` with descriptive context messages
- **Shared state**: Prefer `Arc<Mutex<T>>` for managers
- **Logging**: Use `debug!`, `info!`, `error!`; `eprintln!` only for fatal errors
- **Patterns**: Builder pattern for initialization chains
- **Naming**: Snake_case for functions/variables, PascalCase for types
- **Platform**: `#[cfg(target_os = "...")]` with macOS + Linux fallbacks
- **Formatting**: Run `cargo fmt` and `cargo clippy` before committing
- **MSRV**: Minimum Rust version 1.87 (declared in Cargo.toml)

## TypeScript/React (Frontend)

- **Components**: Functional with TypeScript interfaces
- **Validation**: Zod schemas for type validation
- **Performance**: `useCallback` for stable function references
- **Props**: Destructure with defaults: `disabled = false`
- **Types**: Prefer interface aliases over type aliases for objects
- **Export**: Named exports preferred over default exports
- **Naming**: PascalCase for components, camelCase for functions/variables
- **i18n**: All user-facing strings use i18next (`useTranslation()`)
- **Styling**: Tailwind CSS; path alias: `@/` → `./src/`
- **Imports**: Group: external libs, internal modules, relative; use `import type`

## Error Handling

- **Frontend**: Try/catch with user feedback, rollback optimistic updates
- **Backend**: `?` operator with anyhow context messages
- **Logging**: Log errors with appropriate level

## Commits

- Conventional prefixes: `feat:`, `fix:`, `docs:`, `refactor:`, `chore:`, `test:`
- Focus on _why_, not _what_
- Keep commits atomic and focused

---

# Key Architecture Notes

- **Cross-platform mandatory** — Windows 11 (primary), macOS (first-class), Linux (first-class)
- **Manager pattern** — `managers/` (audio, model, transcription, history, TTS, brain)
- **TTS backends** — `TtsBackend` trait under `tts/backends/` (Piper, Kokoro, Kitten, SAPI, OpenAI, ElevenLabs, Cartesia)
- **Brain** — streaming LLM in `brain/` (SSE client + sentence splitter + TTS bridge)
- **Text pipeline** — `tts/sanitize/` handles ITN, TN, markdown stripping, regex cleanup
- **VAD** — TripleVAD default (RMS → RNNoise → Silero) in `audio_toolkit/vad/`
- **Specta IPC** — typed bindings in `src/bindings.ts`; regenerate with `cargo test export_bindings`
- **WarmEngine** — lifecycle states (Stopped → Loading → WarmingUp → Ready → Error)
- **Single instance** — `tauri_plugin_single_instance` for remote control via CLI flags
- **Settings** — Tauri store plugin with reactive updates and backfill on read
- **Crash logging** — Panics captured to `s2b2s-crash.log` with full backtraces
- **Her loading** — Three.js 3D animation with minimum 3-second display
- **Pipelines** — Dictation, Conversation (STT→Brain→TTS), Read Aloud

---

# File Structure Reference

```
S2B2S/
├── README.md               # Overview, quick start, features
├── S2B2S_REVIEW.md         # Comprehensive analysis document
├── AGENTS.md               # AI assistant guidance
├── BUILD.md                # Build instructions
├── CHANGELOG.md            # Version history
├── CONTRIBUTING.md         # Contributor guidelines
├── CONTRIBUTING_TRANSLATIONS.md  # Translation guide
├── CRUSH.md                # This file — commands + style
├── CLAUDE.md               # AI assistant entry point
├── LICENSE                 # MIT License
├── package.json            # JS dependencies
├── vite.config.ts          # Frontend build config
├── tailwind.config.js      # Tailwind CSS config
├── tsconfig.json           # TypeScript config
├── src/                    # Frontend source
├── src-tauri/              # Backend source (Rust)
├── models/                 # Model files (STT, TTS)
├── scripts/                # Utility scripts
├── tests/                  # E2E tests
├── flake.nix               # Nix flake
└── .github/                # CI/CD workflows and templates
```
