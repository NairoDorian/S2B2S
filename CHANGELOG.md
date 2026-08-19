# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Auto-detect and pull transcribe.cpp upstream changes.** A new
  `scripts/check-transcribe-deps.ts` guard runs before every
  `bun run tauri` invocation (wired into the `tauri` npm script in
  `package.json`). It compares the commit pinned in `Cargo.lock` for the
  `transcribe-cpp` / `transcribe-cpp-sys` git dependencies (forked from
  `NairoDorian/transcribe.cpp`, branch `main`, applied via
  `[patch.crates-io]`) against the remote `main` branch tip via
  `git ls-remote`. When the remote is ahead, it runs
  `cargo update -p transcribe-cpp -p transcribe-cpp-sys` to fetch the latest
  commit, so the next `tauri dev` / `tauri build` automatically recompiles
  the native C++/CMake crate from the new source rather than silently
  reusing a stale cached build. The check is safe by design: it always
  exits `0`, so it never blocks a build, even when offline.
