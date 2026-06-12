# Contributing to S2B2S

Thank you for your interest in contributing to S2B2S! This guide will help you get started with contributing to this open source voice-native desktop application (STT → Brain → TTS).

## Feature Policy

**S2B2S prioritizes stability and bug fixes.** The core STT → Brain → TTS pipeline is feature-complete. New features should gather community support first via [Discussions](https://github.com/NairoDorian/S2B2S/discussions) before a PR is opened. See the roadmap in [README.md](README.md) for features and what's in progress.

**Bug fixes, performance improvements, and cross-platform compatibility are always welcome.**

---

## Philosophy

S2B2S aims to be the most forkable voice-native desktop app. The goal is to create both a useful tool and a foundation for others to build upon — a well-patterned, simple codebase that serves the community. We prioritize:

- **Simplicity**: Clear, maintainable code over clever solutions
- **Extensibility**: Make it easy for others to fork and customize
- **Privacy**: Keep everything local and offline
- **Accessibility**: Free tooling that belongs in everyone's hands
- **Cross-platform**: Windows 11 primary, macOS + Linux first-class

---

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable) — MSRV 1.87
- [Bun](https://bun.sh/) package manager
- Platform-specific build tools (see [BUILD.md](BUILD.md))

### Setting Up Your Development Environment

1. **Fork the repository** on GitHub
2. **Clone your fork**: `git clone git@github.com:YOUR_USERNAME/S2B2S.git`
3. **Add upstream remote**: `git remote add upstream git@github.com:NairoDorian/S2B2S.git`
4. **Install dependencies**: `bun install`
5. **Download required models**:
   ```bash
   mkdir -p src-tauri/resources/models
   curl -o src-tauri/resources/models/silero_vad_v4.onnx https://blob.handy.computer/silero_vad_v4.onnx
   ```
6. **Run in development mode**: `bun run tauri dev` (macOS: `CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev`)

### Understanding the Codebase

S2B2S follows a clean manager-based architecture:

**Backend (Rust — `src-tauri/src/`):**

- `lib.rs` — Main application entry point with Tauri setup
- `managers/` — Core business logic (audio, model, transcription, history, continuous voice)
- `tts/` — Text-to-speech subsystem (7+ backends: Piper, Kokoro, Kitten, SAPI, OpenAI, ElevenLabs, Cartesia)
- `brain/` — Streaming LLM subsystem (SSE client, turn history, sentence splitter, TTS bridge)
- `audio_toolkit/` — Audio processing (capture, VAD, noise suppression, resampling)
- `commands/` — Tauri command handlers
- `shortcut/` — Global keyboard shortcut handling
- `settings.rs` — Application settings management

**Frontend (React/TypeScript — `src/`):**

- `App.tsx` — Main application component with onboarding
- `components/` — React UI components (settings, conversation, model-selector, overlay, etc.)
- `hooks/` — Reusable React hooks
- `stores/` — Zustand state management
- `i18n/` — Internationalization (20 languages)

For more details, see [AGENTS.md](AGENTS.md) and [S2B2S_REVIEW.md](S2B2S_REVIEW.md).

---

## Reporting Bugs

### Before Submitting a Bug Report

1. **Search existing issues** at [github.com/NairoDorian/S2B2S/issues](https://github.com/NairoDorian/S2B2S/issues)
2. **Check discussions** at [github.com/NairoDorian/S2B2S/discussions](https://github.com/NairoDorian/S2B2S/discussions)
3. **Try the latest release** to see if the issue has been fixed
4. **Enable debug mode** (`Cmd/Ctrl+Shift+D`) to gather diagnostic information

### Submitting a Bug Report

Use the [Bug Report template](.github/ISSUE_TEMPLATE/bug_report.md) and include:

**System Information:**

- App version (from Settings → About)
- Operating System (e.g., macOS 14.1, Windows 11, Ubuntu 22.04)
- CPU and GPU

**Bug Details:**

- Clear description
- Steps to reproduce
- Expected vs actual behavior
- Debug logs (from `s2b2s-crash.log` or debug mode)

---

## Suggesting Features

We use GitHub Discussions for feature requests. This keeps issues focused on bugs and actionable tasks.

1. **Search existing discussions** at [github.com/NairoDorian/S2B2S/discussions](https://github.com/NairoDorian/S2B2S/discussions)
2. **Check common feature requests** (Post-processing, Keyboard Shortcuts, etc.)
3. **Create a new discussion** describing the problem, your proposed solution, and alternatives

---

## Making Code Contributions

### Before You Start

1. **Search existing issues and PRs** — both open and closed. Someone may have already addressed this.
2. **Get community feedback for features** — PRs with demonstrated community interest are much more likely to be merged.

### Development Workflow

1. **Create a feature branch**: `git checkout -b feature/your-feature-name` or `fix/your-bug-fix`
2. **Make your changes** following the code style guidelines
3. **Test thoroughly** on your target platform(s)
4. **Commit with conventional commit messages**:
   - `feat:` for new features
   - `fix:` for bug fixes
   - `docs:` for documentation changes
   - `refactor:` for code refactoring
   - `test:` for test additions/changes
   - `chore:` for maintenance tasks
5. **Keep your fork updated**: `git fetch upstream && git rebase upstream/main`
6. **Push and create a Pull Request** — fill out the PR template completely

### Code Style Guidelines

**Rust:**

- Run `cargo fmt` and `cargo clippy` before committing
- Use descriptive names, add doc comments for public APIs
- Handle errors explicitly (avoid `unwrap` in production)
- Cross-platform: always provide macOS + Linux paths alongside Windows

**TypeScript/React:**

- Strict TypeScript, avoid `any` types
- Functional components with hooks
- Tailwind CSS for styling
- All user-facing strings must use i18next
- Path aliases: `@/` → `./src/`

**General:**

- Write self-documenting code
- Add comments for non-obvious logic
- Keep functions small and single-purpose
- Prioritize readability over cleverness

### AI Assistance Disclosure

AI-assisted PRs are welcome! In your PR description, please include:

- Whether AI was used
- Which tools were used (e.g., "Claude Code", "GitHub Copilot", "ChatGPT")
- How extensively it was used

---

## Documentation Contributions

Documentation improvements are valued! You can contribute by:

- Improving README.md, BUILD.md, CONTRIBUTING.md, or AGENTS.md
- Adding code comments and doc comments
- Improving error messages
- Adding to [S2B2S_REVIEW.md](S2B2S_REVIEW.md)

---

## Community Guidelines

- **Be respectful and inclusive** — We welcome contributors of all skill levels
- **Be patient** — Maintained by a small team, responses may take time
- **Be constructive** — Focus on solutions and improvements
- **Search first** — Check existing issues/discussions before creating new ones

---

## Getting Help

- **Discord**: Join our [Discord community](https://discord.com/invite/WVBeWsNXK4)
- **Discussions**: Ask questions in [GitHub Discussions](https://github.com/NairoDorian/S2B2S/discussions)
- **Email**: Reach out at [contact@s2b2s.computer](mailto:contact@s2b2s.computer)

---

## License

By contributing to S2B2S, you agree that your contributions will be licensed under the MIT License. See [LICENSE](LICENSE) for details.

---

**Thank you for contributing to S2B2S!** Your efforts help make speech-to-text and voice-native AI technology more accessible, private, and extensible for everyone.
