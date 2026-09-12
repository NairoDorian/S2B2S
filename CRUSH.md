# Development Commands

> **NOTE:** This is the `Handy_Multi_STT` fork. See [AGENTS.md](AGENTS.md) for
> full architecture details including the Multi-STT branch additions.

**Environment Setup:**

```bash
bun install                    # Install dependencies (postinstall runs scripts/check-nix-deps.ts)
```

There is no VAD model to download — voice activity detection is pure Rust
(Earshot). Speech models come from the in-app catalog on first run.

**Development:**

```bash
bun run tauri dev              # Full app development (via scripts/tauri-runner.ts)
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev  # macOS with cmake fix
bun run dev                    # Frontend only (Vite)
bun run build                  # Build frontend (tsc + vite build)
bun run build:fast             # Release build, CUDA kernels for the local GPU only
bun run build:full             # Release build, full multi-arch CUDA matrix
```

**Checks (run before committing):**

```bash
bun run precommit              # THE gate: meta:sync, meta:check, check:identity,
                               # check:translations, lint, typecheck, format:check, repomix
bun run precommit:full         # the same plus clippy and the Rust test suite
bun run hooks:install          # once per clone: point git at .githooks/
```

The individual steps, if you want to run one on its own:

```bash
bun run typecheck              # tsc -b
bun run lint                   # oxlint (with eslint-plugin-i18next)
bun run format:check           # prettier --check + cargo fmt --check
bun run check:translations     # every locale has exactly en's keys
cd src-tauri && cargo clippy --all-targets && cargo test --all-targets
```

**Maintenance scripts:**

```bash
bun run update                 # rtk → latest, deps with --prerelease, then repomix
bun run update:rtk             # RTK CLI (the agent hook's proxy) to its latest release
bun run update-deps [--prerelease] [--dry-run]   # bump npm + Cargo deps — ALWAYS --prerelease here
bun run repomix                # regenerate repomix-output.xml (also in the gate)
bun scripts/check-transcribe-deps.ts             # re-pin transcribe.cpp fork (also runs before every `tauri` invocation)
bun run update:rtk                               # update the RTK CLI (maintainer tooling)
```

# Code Style Guidelines

**Rust (Backend):**

- Use `anyhow::Error` for error handling with descriptive messages
- Prefer `Arc<Mutex<T>>` for shared state in managers
- Log with `log::{debug, info, warn, error}!`; `eprintln!` only in the
  headless `--transcribe-file` CLI path in `lib.rs`
- Builder pattern for initialization chains
- Snake_case for functions and variables, PascalCase for types
- Edition 2024 — `if let ... && let ...` chains are preferred over nested `if`s

**TypeScript/React (Frontend):**

- Functional components with TypeScript interfaces
- `useCallback` hooks for stable function references
- Destructure props with defaults: `disabled = false`
- Prefer interface aliases over type aliases for objects
- React.FC for explicit component typing
- PascalCase for components, camelCase for variables/functions
- No literal strings in JSX — every user-facing string goes through i18next
  (`oxlint` fails the build otherwise); for literal data use an expression
  container: `{"%APPDATA%/ZER0"}`

**Imports:**

- Group imports: external libs, internal modules, relative imports
- Use type imports for TypeScript: `import type { Settings }`
- Named imports preferred over default exports

**Error Handling:**

- Frontend: Try/catch with user feedback, rollback optimistic updates
- Backend: `?` operator with anyhow context messages
- Log errors appropriately for debugging level

**Component Patterns:**

- Container component pattern for layout
- Composition over inheritance
- Prop drilling minimized with context where appropriate
