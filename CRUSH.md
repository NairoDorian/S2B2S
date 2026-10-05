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
bun run dev:cpu              # CPU-only, every model — the usual working loop
bun run dev:fast             # CUDA kernels for this machine's GPU
bun run dev:full             # full CUDA architecture matrix
bun run tauri dev            # same as dev:fast (the default lane)
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev  # macOS with cmake fix
bun run dev                  # Frontend only (Vite)
bun run build                # Build frontend (tsc -b + vite build)
bun run build:cpu            # Release build, no CUDA at all
bun run build:fast           # Release build, CUDA kernels for the local GPU only
bun run build:full           # Release build, full multi-arch CUDA matrix
```

All of the `dev:*` / `build:*` lanes go through `scripts/tauri-runner.ts`; they
differ only in the CUDA architecture policy, and every lane builds Vulkan. The
lanes are mutually exclusive — `build --fast --cpu` is refused rather than
silently resolved.

**Checks (run before committing):**

```bash
bun run precommit              # THE gate: meta:sync, meta:check, check:identity,
                               # check:translations, lint, typecheck, test:unit,
                               # format:check, repomix
bun run precommit:full         # the same plus clippy and the Rust test suite
bun run hooks:install          # once per clone: point git at .githooks/
```

The individual steps, if you want to run one on its own:

```bash
bun run typecheck              # tsc -b
bun run lint                   # oxlint (with eslint-plugin-i18next)
bun run format:check           # prettier --check + cargo fmt --check
bun run check:translations     # every locale has exactly en's keys
bun run check:model-languages  # every catalog language code maps to one UI language intent
bun run test:unit              # bun test over *.test.ts under src/
bun run test:playwright        # the browser suite in tests/
cd src-tauri && cargo clippy --all-targets && cargo test --all-targets
```

`lint:backend` / `test:backend` run the plain cargo pair. To verify on the CPU
posture instead — minutes faster, and it matches what `dev:cpu` builds — use
`bun scripts/cargo-cpu.ts clippy --all-targets`.

**Maintenance scripts:**

```bash
bun run update                              # rtk → latest, deps with --prerelease, then repomix
bun run update --dry-run                    # report what would move, write nothing
bun run update-deps -- --prerelease         # bump npm + Cargo deps — ALWAYS --prerelease here
bun run update-deps -- --prerelease --dry-run   # preview only, change nothing
bun run repomix                # regenerate repomix-output.xml (also in the gate)
bun run repomix:check          # fail if the pack is older than the newest tracked file
bun run prune:target --dry-run # list the stale src-tauri/target artifacts a run would remove
bun run docs:fetch             # mirror the stack's own docs into docs/vendor/ (docs/STACK_WATCH.md)
bun run meta:sync              # regenerate the identity mirrors after editing scripts/app-meta.ts
bun run check:identity         # fail on a stale or hand-written product name
bun run icons:generate         # regenerate app / tray icons from src/lib/brandMark.ts
bun scripts/check-transcribe-deps.ts        # re-pin the transcribe.cpp fork (also runs before every `tauri` invocation)
```

`bun run update:rtk` updates only the RTK CLI — the maintainer's agent-hook proxy,
not part of the app.

# Code Style Guidelines

**Rust (Backend):**

- Use `anyhow::Error` for error handling with descriptive messages
- Prefer `Arc<Mutex<T>>` for shared state in managers
- Log with `log::{debug, info, warn, error}!`. `eprintln!` is reserved for the
  two places that run before (or outside) the logger: the headless
  `--transcribe-file` CLI path in `lib.rs`, and `portable::init()`, which has to
  report the portable data directory before Tauri is built
- Builder pattern for initialization chains
- Snake_case for functions and variables, PascalCase for types
- Edition 2024 — `if let ... && let ...` chains are preferred over nested `if`s

**TypeScript/Solid (Frontend):**

- Strict TypeScript, avoid `any` types
- Solid 2 components: never destructure props in a component body (it runs
  once — a destructured prop is a mount-time snapshot); read reactive values
  inside JSX bindings, not in component bodies
- Use the `createEffect(compute, apply)` returned-cleanup form; `Dynamic` for
  reactively-switched components
- Prefer interface aliases over type aliases for objects
- PascalCase for components, camelCase for variables/functions
- No literal strings in JSX — every user-facing string goes through i18next
  (`oxlint` fails the build otherwise); for literal data use an expression
  container: `{"—"}` or ``{`#${chunk.index}`}``

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
