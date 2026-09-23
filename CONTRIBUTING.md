# Contributing to ZER0

> **NOTE:** This is the `Handy_Multi_STT` fork. In addition to upstream Handy
> features, this branch adds **Multi-STT** mode — running up to four
> speech-to-text models in parallel with optional LLM-based merge. See
> [AGENTS.md](AGENTS.md) for the full architecture. Contributions to
> Multi-STT features are welcome.

Thank you for your interest in contributing to ZER0! This guide will help you get started with contributing to this open source speech-to-text application.

## ⚠️ Feature Freeze

**ZER0 is a fork with a small maintainer team.** A PR that adds a whole feature the maintainer has not asked for is likely to be declined as it lands; a PR that fixes something, hardens something, or adds a small well-argued capability is welcome. Open an issue or a discussion first if you are unsure which side of that line your change falls on.

**Bug fixes are the top priority.** A working behaviour that broke, a crash, a
stale path or a wrong default is always worth fixing and always welcome.

## 📖 Philosophy

ZER0 aims to be the most forkable speech-to-text app. The goal is to create both a useful tool and a foundation for others to build upon—a well-patterned, simple codebase that serves the community. We prioritize:

- **Simplicity**: Clear, maintainable code over clever solutions
- **Extensibility**: Make it easy for others to fork and customize
- **Privacy**: Keep everything local and offline
- **Accessibility**: Free tooling that belongs in everyone's hands

## 🚀 Getting Started

### Prerequisites

Before you begin, ensure you have the following installed:

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager
- Platform-specific build tools (see [BUILD.md](BUILD.md))

### Setting Up Your Development Environment

1. **Fork the repository** on GitHub

2. **Clone your fork**:

   ```bash
   git clone git@github.com:YOUR_USERNAME/S2B2S.git
   cd S2B2S
   ```

3. **Point your clone at the canonical repository** (your fork is `origin` by
   default; add this as `upstream` so you can rebase onto it):

   ```bash
   git remote add upstream git@github.com:NairoDorian/S2B2S.git
   ```

4. **Install dependencies**:

   ```bash
   bun install
   ```

5. **Models**: nothing to download. Voice activity detection is pure Rust
   (Earshot, no model file) and speech models are fetched from the in-app
   catalog on first run.

6. **Run in development mode**:
   ```bash
   bun run tauri dev
   # On macOS if you encounter cmake errors:
   CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev
   ```

For detailed platform-specific setup instructions, see [BUILD.md](BUILD.md).

### Before You Commit: the Pre-commit Routine

Every change goes through one gate, and you set it up once:

```bash
bun run hooks:install   # once per clone: points git at .githooks/
bun run precommit       # the gate — identity, translations, lint, types,
                        # unit checks, format, and the repomix pack, in ~10 s
bun run precommit:full  # the same, plus clippy and the Rust test suite
```

CI runs these checks plus `check:model-languages` (and the Rust suite), so run
`precommit:full` and `bun run check:model-languages` before a release or a PR
to a release branch.

Two house rules come with it:

- **Every shell command goes through `rtk`** (the token-optimizing CLI proxy) —
  `rtk git …`, `rtk cargo …`, `rtk gh …` — with one exception: `bun` commands
  are never proxied, so `bun run …` is typed as-is.
- **Dependencies are always updated with `--prerelease`**: `bun run update-deps
-- --prerelease`, or just `bun run update`, which also takes the RTK CLI to
  its latest release (`bun run update:rtk`) and regenerates the repomix pack.
  This project deliberately tracks the newest published version of every
  dependency, so the next release is tested against what is actually newest.

**Never spell the product name by hand.** It lives in `scripts/app-meta.ts` and
its generated mirrors; `bun run check:identity` fails the build when a stale
name survives. `bun run meta:sync` after editing `app-meta.ts`.

### Understanding the Codebase

ZER0 follows a clean architecture pattern:

**Backend (Rust - `src-tauri/src/`):**

- `lib.rs` - Main application entry point with Tauri setup
- `managers/` - Core business logic (audio, model, transcription)
  - `transcription.rs` - Includes `extra_engines` HashMap for Multi-STT parallel model inference
- `audio_toolkit/` - Low-level audio processing (recording, VAD)
- `commands/` - Tauri command handlers for frontend communication
- `shortcut/mod.rs` - Global keyboard shortcut handling (includes multi-STT shortcut registration)
- `actions.rs` - Shortcut actions (`TranscribeAction`, `MultiSttAction`, `CancelAction`) — `MultiSttAction` runs parallel multi-model transcription with optional LLM merge
- `llm_client.rs` - LLM API client for post-processing and multi-STT merge
- `settings.rs` - Application settings management (includes multi-STT settings)

**Frontend (Solid 2/TypeScript - `src/`):**

- `App.tsx` - Main application component
- `components/` - Solid UI components
  - `settings/multi-stt/MultiSttSettings.tsx` - Multi-STT configuration UI
- `hooks/` - Reusable Solid hooks (`useSettings`, `useOsType`)
- `lib/types/events.ts` - Shared TypeScript event payload types
- `stores/settingsStore.ts` - Settings store; every settings key needs a `settingUpdaters` entry
- `stores/modelStore.ts` - Model store: list, select, download, cancel and delete models

For more details, see the Architecture section in [README.md](README.md) or [AGENTS.md](AGENTS.md).

## 🐛 Reporting Bugs

### Before Submitting a Bug Report

1. **Search existing issues** at [github.com/NairoDorian/S2B2S/issues](https://github.com/NairoDorian/S2B2S/issues)
2. **Check discussions** at [github.com/NairoDorian/S2B2S/discussions](https://github.com/NairoDorian/S2B2S/discussions)
3. **Try the latest release** to see if the issue has been fixed
4. **Enable debug mode** (`Cmd/Ctrl+Shift+D`) to gather diagnostic information

### Submitting a Bug Report

When creating a bug report, please include:

**System Information:**

- App version (found in settings or about section)
- Operating System (e.g., macOS 14.1, Windows 11, Ubuntu 22.04)
- CPU (e.g., Apple M2, Intel i7-12700K, AMD Ryzen 7 5800X)
- GPU (e.g., Apple M2 GPU, NVIDIA RTX 4080, Intel UHD Graphics)

**Bug Details:**

- Clear description of the bug
- Steps to reproduce
- Expected behavior
- Actual behavior
- Screenshots or logs if applicable
- Information from debug mode if relevant

Use the [Bug Report template](.github/ISSUE_TEMPLATE/bug_report.md) when creating an issue.

## 💡 Suggesting Features

We use GitHub Discussions for feature requests rather than issues. This keeps issues focused on bugs and actionable tasks while allowing more open-ended conversations about features.

### Before Suggesting a Feature

1. **Search existing discussions** at [github.com/NairoDorian/S2B2S/discussions](https://github.com/NairoDorian/S2B2S/discussions)
2. **Check common feature requests**:
   - Browse the open discussions for the topic you have in mind; the upstream project's threads on post-processing and hotkeys are worth reading for background but are not mirrored here.

### Submitting a Feature Request

1. Go to [Discussions](https://github.com/NairoDorian/S2B2S/discussions)
2. Click "New discussion"
3. Choose the appropriate category (Ideas, Feature Requests, etc.)
4. Describe your feature idea including:
   - The problem you're trying to solve
   - Your proposed solution
   - Any alternatives you've considered
   - How it fits with ZER0's philosophy

## 🔧 Making Code Contributions

### Before You Start

**This is critical:** Before writing any code, please do the following:

1. **Search existing issues and PRs** - Check both open AND closed issues and pull requests. Someone may have already addressed this, or there may be a reason it was closed.
   - [Open issues](https://github.com/NairoDorian/S2B2S/issues)
   - [Closed issues](https://github.com/NairoDorian/S2B2S/issues?q=is%3Aissue+is%3Aclosed)
   - [Open PRs](https://github.com/NairoDorian/S2B2S/pulls)
   - [Closed PRs](https://github.com/NairoDorian/S2B2S/pulls?q=is%3Apr+is%3Aclosed)

2. **If something was previously closed** - If you want to revisit a closed issue or PR, you need to:
   - Provide a strong argument for why it should be reconsidered
   - Gather community feedback first via [Discussions](https://github.com/NairoDorian/S2B2S/discussions)
   - Link to that discussion in your PR

3. **Get community feedback for features** - PRs with demonstrated community interest are **much more likely to be merged**. Start a discussion, get feedback, and link to it in your PR. This helps ensure ZER0 stays focused and useful for the most people without becoming bloated.

Community feedback is essential to keeping ZER0 the best it can be for everyone. It helps prioritize what matters most and prevents feature creep.

### Development Workflow

1. **Create a feature branch**:

   ```bash
   git checkout -b feature/your-feature-name
   # or
   git checkout -b fix/your-bug-fix
   ```

2. **Make your changes**:
   - Write clean, maintainable code
   - Follow existing code style and patterns
   - Add comments for complex logic
   - Keep commits focused and atomic

3. **Test thoroughly**:
   - Test on your target platform(s)
   - Verify existing functionality still works
   - Test edge cases and error conditions
   - Use debug mode to verify audio/transcription behavior

4. **Commit your changes**:

   ```bash
   git add .
   git commit -m "feat: add your feature description"
   # or
   git commit -m "fix: describe the bug fix"
   ```

   Use conventional commit messages:
   - `feat:` for new features
   - `fix:` for bug fixes
   - `docs:` for documentation changes
   - `refactor:` for code refactoring
   - `test:` for test additions/changes
   - `chore:` for maintenance tasks

5. **Keep your fork updated**:

   ```bash
   git fetch upstream
   git rebase upstream/main
   ```

6. **Push to your fork**:

   ```bash
   git push origin feature/your-feature-name
   ```

7. **Create a Pull Request**:
   - Go to the [ZER0 repository](https://github.com/NairoDorian/S2B2S)
   - Click "New Pull Request"
   - Select your fork and branch
   - Fill out the PR template completely, including:
     - Clear description of changes
     - Links to related issues or discussions
     - **Community feedback** (especially important for features)
     - How you tested the changes
     - Screenshots/videos if applicable
     - Breaking changes (if any)

   **Remember:** PRs with community support are prioritized. If you haven't already, start a [discussion](https://github.com/NairoDorian/S2B2S/discussions) to gather feedback before or alongside your PR. It is not explicitly required to gather feedback, but it certainly helps your PR get merged faster.

### AI Assistance Disclosure

**AI-assisted PRs are welcome!** Use whatever tools help you contribute, just be upfront about it.

In your PR description, please include:

- Whether AI was used (yes/no)
- Which tools were used (e.g., "Claude Code", "GitHub Copilot", "ChatGPT")
- How extensively it was used (e.g., "generated boilerplate", "helped debug", "wrote most of the code")

### Code Style Guidelines

**Rust:**

- Follow standard Rust formatting (`cargo fmt`)
- Run `cargo clippy` and address warnings
- Use descriptive variable and function names
- Add doc comments for public APIs
- Handle errors explicitly (avoid unwrap in production code)

**TypeScript/Solid 2:**

- Use TypeScript strictly, avoid `any` types
- Never destructure props in a component body: the body runs once, so a
  destructured prop is a mount-time snapshot
- Read reactive values inside JSX bindings, not in component bodies
- Use the `createEffect(compute, apply)` form and return the cleanup from
  `apply`; use `Dynamic` for reactively-switched components
- Keep components small and focused
- Use Tailwind CSS for styling

**General:**

- Write self-documenting code
- Add comments for non-obvious logic
- Keep functions small and single-purpose
- Prioritize readability over cleverness

### Testing Your Changes

**Manual Testing:**

- Run the app in development mode: `bun run tauri dev`
- Test your changes with debug mode enabled
- Verify on multiple platforms if possible
- Test with different audio devices
- Try various transcription scenarios

**Automated checks (CI runs these):**

```bash
bun run typecheck            # tsc
bun run test:unit            # bun test over *.test.ts under src/
bun run lint                 # oxlint + i18next/no-literal-string
bun run format:check         # prettier + cargo fmt
bun run check:translations   # locale key parity with en
bun run check:model-languages # catalog language codes map to the UI's languages
cd src-tauri && cargo clippy --all-targets && cargo test --all-targets
```

**Building for Production:**

```bash
bun run build:fast   # local GPU only — quick release build for testing
bun run build:full   # full multi-architecture CUDA matrix for distribution
```

Test the production build to ensure it works as expected.

## 📝 Documentation Contributions

Documentation improvements are highly valued! You can contribute by:

- Improving README.md, BUILD.md, or this CONTRIBUTING.md
- Adding code comments and doc comments
- Creating tutorials or guides
- Improving error messages
- Updating the project website content

## 🤝 Community Guidelines

- **Be respectful and inclusive** - We welcome contributors of all skill levels
- **Be patient** - This is maintained by a small team, responses may take time
- **Be constructive** - Focus on solutions and improvements
- **Be collaborative** - Help others and share knowledge
- **Search first** - Check existing issues/discussions before creating new ones

## 🎯 Good First Issues

Look for issues labeled `good first issue` or `help wanted` if you're new to the project. These are typically:

- Well-defined and scoped
- Good for learning the codebase
- Mentor support available

## 📞 Getting Help

- **Discussions**: Ask questions in [GitHub Discussions](https://github.com/NairoDorian/S2B2S/discussions)
- **Issues**: File a bug or a question at [github.com/NairoDorian/S2B2S/issues](https://github.com/NairoDorian/S2B2S/issues)

## 📜 License

By contributing to ZER0, you agree that your contributions will be licensed under the MIT License. See [LICENSE](LICENSE) for details.

---

**Thank you for contributing to ZER0!** Your efforts help make speech-to-text technology more accessible, private, and extensible for everyone.
