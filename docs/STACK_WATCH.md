# Stack Watch — keeping up with the technologies underneath the app

ZER0 is built on tools that move: Solid 2 is a release candidate, Tauri (the
app is on the 3.x alpha) and Bun cut releases weekly, Tailwind and Oxlint ship continuously, and the
transcribe.cpp fork is pinned to a moving `main`. This project's own
conventions (AGENTS.md) are authoritative for how we write code, but the
frameworks underneath it can hand us improvements we have not imagined — or
deprecate something this codebase already relies on. The only way to notice
either is to read what they publish, refreshed and diffed locally.

## The mirror

`bun run docs:fetch` (`scripts/fetch-stack-docs.ts`) mirrors the documentation
of every piece of the stack into `docs/vendor/<source>/` as plain Markdown:

| Source           | Covers                                                                                                 |
| ---------------- | ------------------------------------------------------------------------------------------------------ |
| `solidjs`        | Solid 2 wiki (v2.solidjs.com), scraped to clean Markdown                                               |
| `solidjs-src`    | the `.mdx` sources behind that wiki (`solidjs/solid-docs`, branch `v2-rebuild`)                        |
| `tauri`          | Tauri docs (`tauri-apps/tauri-docs` HEAD, which documents v2 — read API details against the v3 branch) |
| `bun`            | Bun's docs (in-repo under `docs/`)                                                                     |
| `tailwind`       | Tailwind v4 docs (`tailwindlabs/tailwindcss.com`)                                                      |
| `oxlint`         | Oxlint docs (`oxc-project/website`)                                                                    |
| `transcribe-cpp` | the transcribe.cpp fork's `docs/` (the inference runtime)                                              |
| `tauri-specta`   | the typed-bindings generator behind `src/bindings.ts`                                                  |

Every mirrored file carries frontmatter naming its upstream source, so
anything quoted here can be checked against the canonical document. Each
source folder has an `INDEX.md`; `docs/vendor/INDEX.md` summarizes all of
them. The mirror is gitignored — it is a local reference cache, not part of
the repository, the build or any gate — and `docs/vendor/.state.json` holds
the per-file content hashes from the previous run, which is what makes
change detection possible without git.

Adding a source is one entry in the `SOURCES` array of
`scripts/fetch-stack-docs.ts`: a site to scrape (server-rendered pages only)
or a GitHub repo + path prefix whose `.md`/`.mdx` tree should be mirrored.

## The loop

1. **Refresh** — `bun run docs:fetch`. The run prints, per source, how many
   pages are new, changed and removed since the last run, and names the
   upstream failures rather than swallowing them.
2. **Read the deltas** — open the changed pages in `docs/vendor/`. This is
   the step that produces value: not the mirror, the reading.
3. **Evaluate against the app** — for each notable change, ask the only
   question that matters here: _does this improve ZER0, or does it break
   something we do?_ Concretely:
   - a new API that removes code we hand-rolled (a plugin, a helper, a shim),
   - a performance recommendation on a path we care about (see
     `docs/PERFORMANCE.md` before acting — our latency budget outranks most
     generic advice),
   - a deprecation of something we use (then: migration note in
     `docs/KNOWN_ISSUES.md`, fix before the next dependency update),
   - a corrected behaviour we had worked around.
4. **Fold it in** — real improvements go through the normal workflow (issue
   or direct change, `bun run precommit`, conventional commit). Nothing
   enters the app just because upstream shipped it; the app is real-time and
   allocation-sensitive in ways most advice is not written for.
5. **Keep the mirror honest** — run `bun run docs:fetch` before starting a
   release (`bun run precommit:routine` is a natural moment), and whenever an
   agent works on an area whose framework has just moved.

The timing rule mirrors the dependency policy: refresh and read _before_ the
work that could benefit, not after the release that shipped without it.

## What the agent should do with it

When picking up work in an area (a Solid component, a Tauri command, a Bun
script), an agent should treat the corresponding `docs/vendor/` folder as the
first reference to consult — it is local, current and searchable, so it
beats web lookups and beats training-data memory of an API that may have
moved. Where the scraped wiki and the `.mdx` source disagree, the source repo
wins; where both disagree with the behaviour of the installed version, the
installed version wins and the discrepancy is worth a KNOWN_ISSUES line.
