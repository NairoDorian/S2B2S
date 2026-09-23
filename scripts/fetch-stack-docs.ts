// scripts/fetch-stack-docs.ts
//
// Mirror the documentation of the technologies this project is built on into
// `docs/vendor/<source>/` as plain Markdown, and report what changed since the
// last run. This is the grounding step of the stack-watch loop: the app's own
// conventions are authoritative, but the frameworks underneath it move — Solid
// 2 is an RC, Tauri (on its 3.x alpha here) and Bun release weekly — and the
// only way to notice an improvement (or a deprecation this codebase already
// commits) is to read what the projects publish, refreshed and diffed locally.
//
// The loop this serves (see docs/STACK_WATCH.md):
//   bun run docs:fetch          refresh the mirrors, print new/changed pages
//   git-independent diffing     .state.json holds per-file hashes between runs
//   then read the deltas        evaluate them against the app, fold in what helps
//
// Two fetcher kinds, because doc sites publish two ways:
//
//   site    scrape a server-rendered docs site (the SolidJS v2 wiki, a
//           SolidStart app whose pages ship full content in the initial HTML).
//           Ported from the standalone scraper at PROJECTS\SolidJS2.0 — the
//           expressive-code `data-code` recovery, callout and details mapping
//           and provenance frontmatter all come from there.
//
//   github  mirror a repo's markdown tree through the git trees API +
//           raw.githubusercontent.com (Tauri's and Bun's docs live in-repo as
//           .mdx/.md). One API call per repo for the listing; file bodies come
//           from the raw host and are not rate-limited the way the API is.
//           MDX is kept as-is apart from frontmatter stripping — these files
//           are reference material, not build inputs, so the JSX components a
//           few pages use are left visible rather than half-converted.
//
// Output layout:
//   docs/vendor/<source>/...        one .md per page, mirroring the URL path
//   docs/vendor/<source>/INDEX.md   every page of that source, grouped
//   docs/vendor/INDEX.md            the sources, page counts, last refresh
//   docs/vendor/.state.json         content hashes from the previous run —
//                                   the diff baseline. Gitignored with the rest.
//
// When it runs: only when asked (`bun run docs:fetch`). Never in a gate or a
// build — it needs the network, and a mirror that cannot refresh must never
// block one that can build. Politeness: small concurrency, per-request delays,
// retry with backoff, a descriptive User-Agent.

import { createHash } from "crypto";
import { mkdir, readFile, rm, writeFile } from "fs/promises";
import { join, dirname, resolve } from "path";
import * as cheerio from "cheerio";
import TurndownService from "turndown";
import { gfm } from "turndown-plugin-gfm";
import { APP, REPO_URL } from "./app-meta";

const TAG = "[docs]";
const ROOT = resolve(import.meta.dirname, "..");
const VENDOR_DIR = "docs/vendor";

// ---------------------------------------------------------------------------
// sources
// ---------------------------------------------------------------------------

interface SiteSource {
  kind: "site";
  id: string;
  base: string;
  /** Human note about what this mirror covers and why it matters to the app. */
  note: string;
}

interface GitHubSource {
  kind: "github";
  id: string;
  repo: string;
  /**
   * Branch/tag to mirror. Defaults to the repo's HEAD — set it when the docs
   * of interest live on a named branch (SolidJS keeps the v2 wiki on
   * `v2-rebuild` while `main` still serves the 1.x docs).
   */
  ref?: string;
  /** Only paths under this prefix are mirrored (the repo's docs folder). */
  prefix: string;
  note: string;
}

type Source = SiteSource | GitHubSource;

const SOURCES: Source[] = [
  {
    kind: "site",
    id: "solidjs",
    base: "https://v2.solidjs.com",
    note: "Solid 2 — the frontend framework (RC): reactivity, components, stores, migration guides. Clean Markdown scraped from the SSR site.",
  },
  {
    kind: "github",
    id: "solidjs-src",
    repo: "solidjs/solid-docs",
    ref: "v2-rebuild",
    prefix: "src/routes",
    note: "Solid 2's canonical docs repo — the .mdx sources the v2 wiki is built from (branch v2-rebuild; main still serves the 1.x docs). Read this when the scraped page leaves a question open.",
  },
  {
    kind: "github",
    id: "tauri",
    repo: "tauri-apps/tauri-docs",
    prefix: "src/content/docs",
    note: "Tauri — windowing, IPC, plugins, permissions, bundling: the shell the app runs in. The app is on the 3.x alpha; tauri-docs HEAD documents v2, so read API details against the v3 branch.",
  },
  {
    kind: "github",
    id: "bun",
    repo: "oven-sh/bun",
    prefix: "docs",
    note: "Bun — runtime, bundler, test runner, package manager: the toolchain behind every script.",
  },
  {
    kind: "github",
    id: "tailwind",
    repo: "tailwindlabs/tailwindcss.com",
    prefix: "src/docs",
    note: "Tailwind CSS v4 — the styling system (source('.') scanning, utilities, @theme).",
  },
  {
    kind: "github",
    id: "oxlint",
    repo: "oxc-project/website",
    prefix: "src/docs",
    note: "Oxlint — the linter the precommit gate runs (rules, config, the eslint-plugin-i18next integration).",
  },
  {
    kind: "github",
    id: "transcribe-cpp",
    repo: "NairoDorian/transcribe.cpp",
    prefix: "docs",
    note: "The transcribe.cpp fork — the inference runtime behind every model (streaming, CUDA, latency presets).",
  },
  {
    kind: "github",
    id: "tauri-specta",
    repo: "oscartbeaumont/tauri-specta",
    prefix: "",
    note: "tauri-specta — the typed bindings generator behind src/bindings.ts.",
  },
];

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

const args = process.argv.slice(2);
const flag = (name: string): string | undefined => {
  const i = args.indexOf(`--${name}`);
  if (i === -1) return undefined;
  const value = args[i + 1];
  return value && !value.startsWith("--") ? value : "true";
};

const ONLY = flag("source");
const LIMIT = Number(flag("limit") ?? 0) || 0;

const selected = ONLY ? SOURCES.filter((s) => s.id === ONLY) : SOURCES;
if (ONLY && selected.length === 0) {
  console.error(
    `${TAG} unknown source "${ONLY}". Known: ${SOURCES.map((s) => s.id).join(", ")}.`,
  );
  process.exit(1);
}

const SCRAPED_ON = new Date().toLocaleDateString("en-CA"); // local date, YYYY-MM-DD

// ---------------------------------------------------------------------------
// shared fetching plumbing
// ---------------------------------------------------------------------------

const HEADERS = {
  "User-Agent": `Mozilla/5.0 (compatible; ${APP.slug}-stack-docs/1.0; +${REPO_URL})`,
  Accept: "text/html,application/xml,text/plain,application/json",
};

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function fetchText(url: string, attempt = 1): Promise<string> {
  try {
    const res = await fetch(url, { headers: HEADERS, redirect: "follow" });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return await res.text();
  } catch (err) {
    if (attempt >= 3) throw err;
    await sleep(400 * attempt);
    return fetchText(url, attempt + 1);
  }
}

/** Run `worker` over `items`, at most `size` at a time, in item order. */
async function runPool<T, R>(
  items: T[],
  worker: (item: T, index: number) => Promise<R>,
  size: number,
  pauseMs = 80,
): Promise<R[]> {
  const results = new Array<R>(items.length);
  let cursor = 0;
  const runners = Array.from(
    { length: Math.min(size, items.length) },
    async () => {
      while (cursor < items.length) {
        const i = cursor++;
        results[i] = await worker(items[i], i);
        await sleep(pauseMs);
      }
    },
  );
  await Promise.all(runners);
  return results;
}

const sha256 = (text: string) =>
  createHash("sha256").update(text, "utf8").digest("hex");

// ---------------------------------------------------------------------------
// kind: site — server-rendered docs pages (SolidJS v2)
// ---------------------------------------------------------------------------

// expressive-code encodes newlines inside its copy-to-clipboard attribute as a
// control character — decoding it reproduces the source byte for byte.
// Unresolved: expressive-code's own copy script names U+007F (DEL), while this
// constant has always held U+001F (Unit Separator), written here as an escape
// so the value is visible. The mirrored pages contain neither, so the
// `.ec-line` fallback is what produces the code blocks today; check one
// `data-code` attribute before changing it.
const DEL = "\u001f";

const turndown = new TurndownService({
  headingStyle: "atx",
  hr: "---",
  bulletListMarker: "-",
  codeBlockStyle: "fenced",
  fence: "```",
  emDelimiter: "*",
  strongDelimiter: "**",
  linkStyle: "inlined",
  br: "\n",
});
turndown.use(gfm);

// Drop leftover link targets that point at the docs repo chrome.
turndown.addRule("dropRepoChromeLinks", {
  filter: (node) =>
    node.nodeName === "A" &&
    /github\.com\/solidjs\/solid-docs/.test(node.getAttribute("href") || ""),
  replacement: () => "",
});

// Collapse the many empty spans/divs the framework leaves behind.
turndown.addRule("dropEmptyInline", {
  filter: (node) =>
    ["SPAN", "DIV"].includes(node.nodeName) &&
    !node.textContent.trim() &&
    !node.querySelector("img, pre, code, br, hr"),
  replacement: () => "",
});

/** Callout background colour -> GitHub alert kind. */
const CALLOUT_KINDS: [RegExp, string][] = [
  [/^amber|^yellow/, "WARNING"],
  [/^orange|^red|^rose/, "CAUTION"],
  [/^emerald|^green|^teal|^lime/, "TIP"],
  [/^blue|^sky|^indigo|^violet|^purple|^slate|^gray|^zinc/, "NOTE"],
];

function calloutKind(tone: string): string {
  for (const [re, kind] of CALLOUT_KINDS) if (re.test(tone)) return kind;
  return "NOTE";
}

function decodeCodeAttr(raw: string): string {
  return raw.split(DEL).join("\n");
}

/** Turn every expressive-code figure into a plain <pre><code> for turndown. */
function normalizeCodeBlocks($) {
  $("pre").each((_, pre) => {
    const $pre = $(pre);
    const frame = $pre.closest("figure");
    const scope = frame.length ? frame : $pre;

    const lang = $pre.attr("data-language") || "";
    const rawAttr = scope.find("[data-code]").attr("data-code");

    let code: string;
    if (rawAttr != null) {
      code = decodeCodeAttr(rawAttr);
    } else {
      // Fallback: stitch the rendered lines back together.
      const lines = $pre
        .find(".ec-line")
        .map((__, line) => $(line).text())
        .get();
      code = lines.length ? lines.join("\n") : $pre.text();
    }

    // A "has-title" frame shows e.g. the filename above the snippet.
    const title = frame.length
      ? frame.find("figcaption .title").first().text().trim()
      : "";

    const replacement = $("<pre><code></code></pre>");
    replacement
      .find("code")
      .attr(
        "class",
        lang ? `language-${lang === "text" ? "" : lang}`.trim() : "",
      )
      .text(code);

    const node = frame.length ? frame : $pre;
    if (title) {
      const $caption = $("<p></p>").append($("<em></em>").text(title));
      node.replaceWith($("<div></div>").append($caption).append(replacement));
    } else {
      node.replaceWith(replacement);
    }
  });
}

/** Callout boxes -> GitHub alerts, keeping their title and body. */
function normalizeCallouts($) {
  $('div[class*="rounded-3xl"]').each((_, el) => {
    const $el = $(el);
    const cls = $el.attr("class") || "";
    if (!/\bp-4\b/.test(cls) || !/\bbg-/.test(cls)) return;

    const tone = (cls.match(/\bbg-([a-z]+)-/) ||
      ([, ""] as RegExpMatchArray))[1];
    const kind = calloutKind(tone);

    // The heading span sits as a sibling of the body's `.prose` wrapper.
    const $title = $el
      .find("span")
      .filter((__, s) => /(^|\s)text-xl(\s|$)/.test($(s).attr("class") || ""))
      .first();

    const $body = $el.find(".prose").first();

    const $bq = $("<blockquote></blockquote>");
    $bq.append($("<p></p>").text(`[!${kind}]`));
    if ($title.length) {
      $bq.append(
        $("<p></p>").append($("<strong></strong>").text($title.text().trim())),
      );
    }
    if ($body.length) $bq.append($body.contents());

    $el.replaceWith($bq);
  });
}

/** <details>/<summary> accordions -> blockquote with a bold lead-in. */
function normalizeDetails($) {
  $("details").each((_, el) => {
    const $el = $(el);
    const $summary = $el.children("summary").first();

    // The summary nests a wrapper span around the label spans, so take leaves
    // only — otherwise .text() on the wrapper repeats its children's text.
    const lead = $summary
      .find("span")
      .filter((__, s) => $(s).find("span").length === 0)
      .map((__, s) => $(s).text().trim())
      .get()
      .filter(Boolean)
      .join(" — ");
    const summaryText = (lead || $summary.text()).trim();

    $summary.remove();

    const $bq = $("<blockquote></blockquote>");
    if (summaryText) {
      $bq.append($("<p></p>").append($("<strong></strong>").text(summaryText)));
    }
    $bq.append($el.contents());

    $el.replaceWith($bq);
  });
}

/** Strip site chrome so only the actual document body survives. */
function stripChrome($, $article) {
  $article.find("script, style, noscript, svg, button, nav").remove();

  // The "Edit this page" link is a useful provenance breadcrumb.
  const github = $article
    .find('a[href*="github.com/solidjs/solid-docs/edit/"]')
    .first()
    .attr("href");

  $article
    .find("span")
    .filter((__, s) =>
      /^(Last updated:|Edit this page|Report an issue)/.test(
        $(s).text().trim(),
      ),
    )
    .remove();

  return github ? github.split("?")[0] : null;
}

interface SitePage {
  title: string;
  markdown: string;
  github: string | null;
  section: string | null;
}

function articleToMarkdown(html: string, url: string): SitePage | null {
  const $ = cheerio.load(html);
  const $article = $("article").first();
  if (!$article.length) return null;

  const github = stripChrome($, $article);

  // Some pages lead with a breadcrumb label ("Stores") before the <h1>. Keep it
  // as frontmatter rather than letting it float loose above the title.
  let section: string | null = null;
  const $crumb = $article
    .children("span")
    .filter(
      (__, s) => $(s).children().length === 0 && $(s).text().trim().length < 60,
    )
    .first();
  if ($crumb.length && $crumb.nextAll("h1").length) {
    section = $crumb.text().trim();
    $crumb.remove();
  }

  // Headings wrap their text in an anchor; unwrap so we don't emit [text](#id).
  $article.find("h1, h2, h3, h4, h5, h6").each((_, h) => {
    const $h = $(h);
    $h.find("a").each((__, a) => $(a).replaceWith($(a).contents()));
  });

  normalizeCodeBlocks($);
  normalizeCallouts($);
  normalizeDetails($);

  // Rewrite site-relative links to absolute so they work from anywhere.
  $article.find("a[href^='/']").each((_, a) => {
    const href = $(a).attr("href");
    if (href) $(a).attr("href", new URL(href, url).toString());
  });

  const title =
    $article.find("h1").first().text().trim() || $("title").text().trim();
  const markdown = turndown
    .turndown($article.html() || "")
    // Turndown escapes the brackets in `[!NOTE]`, which breaks GitHub's alert
    // syntax. Put the marker back the way GitHub expects to see it.
    .replace(/\\\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\\\]/g, "[!$1]");

  return { title, markdown, github, section };
}

/** Sitemap page list; the <loc> hosts point at the deploy host, so keep only paths. */
async function urlsFromSitemap(base: string): Promise<string[]> {
  const xml = await fetchText(`${base}/sitemap.xml`);
  const locs = [...xml.matchAll(/<loc>\s*([^<\s]+)\s*<\/loc>/g)].map(
    (m) => m[1],
  );
  const paths = locs
    .map((loc) => {
      try {
        return new URL(loc).pathname;
      } catch {
        return null;
      }
    })
    .filter((p): p is string => p !== null);
  return [...new Set(paths)].map((p) => base + p);
}

function siteRelPath(url: string): string {
  const { pathname } = new URL(url);
  const clean = decodeURIComponent(pathname).replace(/^\/+|\/+$/g, "");
  return clean ? `${clean}.md` : "index.md";
}

async function scrapeSite(source: SiteSource): Promise<Map<string, string>> {
  console.log(`${TAG} ${source.id}: reading sitemap of ${source.base} …`);
  const urls = await urlsFromSitemap(source.base);
  const work = LIMIT ? urls.slice(0, LIMIT) : urls;
  console.log(`${TAG} ${source.id}: ${work.length} pages.`);

  const pages = new Map<string, string>();
  const failures: { url: string; error: string }[] = [];
  let done = 0;

  await runPool(
    work,
    async (url) => {
      const rel = siteRelPath(url);
      try {
        const html = await fetchText(url);
        const parsed = articleToMarkdown(html, url);
        if (!parsed) throw new Error("no <article> in response");

        const provenance = [
          "---",
          `title: "${parsed.title.replace(/"/g, '\\"')}"`,
          parsed.section
            ? `section: "${parsed.section.replace(/"/g, '\\"')}"`
            : null,
          `source: "${url}"`,
          parsed.github
            ? `github_source: "${parsed.github.split("?")[0]}"`
            : null,
          `scraped: "${SCRAPED_ON}"`,
          "---",
          "",
        ]
          .filter((l) => l !== null)
          .join("\n");

        pages.set(rel, `${provenance}${parsed.markdown.trim()}\n`);
      } catch (err) {
        failures.push({
          url,
          error: err instanceof Error ? err.message : String(err),
        });
      }
      done++;
      if (done % 20 === 0 || done === work.length) {
        console.log(`${TAG} ${source.id}: ${done}/${work.length} pages`);
      }
    },
    4,
  );

  if (failures.length) {
    console.log(`${TAG} ${source.id}: ${failures.length} page(s) failed:`);
    for (const f of failures.slice(0, 10))
      console.log(`${TAG}   ${f.url} (${f.error})`);
  }
  return pages;
}

// ---------------------------------------------------------------------------
// kind: github — mirror a repo's markdown tree
// ---------------------------------------------------------------------------

interface GitHubFile {
  path: string;
  sha: string;
  size: number;
}

async function listGitHubMarkdown(source: GitHubSource): Promise<GitHubFile[]> {
  const ref = source.ref ?? "HEAD";
  const api = `https://api.github.com/repos/${source.repo}/git/trees/${ref}?recursive=1`;
  const raw = await fetchText(api);
  const tree = JSON.parse(raw) as {
    truncated?: boolean;
    tree: { type: string; path: string; size?: number }[];
  };
  if (tree.truncated) {
    throw new Error(
      `${source.repo}: the git tree API truncated its answer; the mirror would be partial.`,
    );
  }
  const prefix = source.prefix ? `${source.prefix.replace(/\/+$/, "")}/` : "";
  return tree.tree
    .filter(
      (x) =>
        x.type === "blob" &&
        x.path.startsWith(prefix) &&
        (x.path.endsWith(".md") || x.path.endsWith(".mdx")),
    )
    .map((x) => ({ path: x.path, sha: x.sha, size: x.size ?? 0 }));
}

/** Remove an MDX file's own frontmatter; provenance is re-added uniformly. */
function stripFrontmatter(text: string): string {
  const match = text.match(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/);
  return match ? text.slice(match[0].length) : text;
}

async function mirrorGitHub(
  source: GitHubSource,
): Promise<Map<string, string>> {
  const files = await listGitHubMarkdown(source);
  const work = LIMIT ? files.slice(0, LIMIT) : files;
  const bytes = work.reduce((s, f) => s + f.size, 0);
  console.log(
    `${TAG} ${source.id}: ${work.length} markdown file(s), ${(bytes / 1024 / 1024).toFixed(1)} MiB from ${source.repo}.`,
  );

  const pages = new Map<string, string>();
  const failures: { path: string; error: string }[] = [];
  let done = 0;

  await runPool(
    work,
    async (file) => {
      // One failed file is reported and skipped, as `scrapeSite` does for a
      // page — it must not abort the whole source after its retries.
      try {
        const ref = source.ref ?? "HEAD";
        const raw = await fetchText(
          `https://raw.githubusercontent.com/${source.repo}/${ref}/${file.path}`,
        );
        const prefix = source.prefix
          ? `${source.prefix.replace(/\/+$/, "")}/`
          : "";
        const rel =
          file.path
            .slice(prefix.length)
            // SolidJS's docs repo encodes page order as `(0)`, `(1)`… folder
            // and file prefixes; strip them so the mirror is readable paths.
            .split("/")
            .map((segment) => segment.replace(/^\(\d+\)/, ""))
            .join("/")
            .replace(/\.mdx?$/, "") + ".md";
        const provenance = [
          "---",
          `source_repo: "${source.repo}"`,
          `source_path: "${file.path}"`,
          `source: "https://github.com/${source.repo}/blob/${ref}/${file.path}"`,
          `scraped: "${SCRAPED_ON}"`,
          "---",
          "",
        ].join("\n");
        pages.set(
          rel,
          `${provenance}${stripFrontmatter(raw).replace(/\r\n/g, "\n").trim()}\n`,
        );
      } catch (err) {
        failures.push({
          path: file.path,
          error: err instanceof Error ? err.message : String(err),
        });
      }
      done++;
      if (done % 40 === 0 || done === work.length) {
        console.log(`${TAG} ${source.id}: ${done}/${work.length} files`);
      }
    },
    6,
    40,
  );

  if (failures.length) {
    console.log(`${TAG} ${source.id}: ${failures.length} file(s) failed:`);
    for (const f of failures.slice(0, 10))
      console.log(`${TAG}   ${f.path} (${f.error})`);
  }
  return pages;
}

// ---------------------------------------------------------------------------
// state — the diff baseline between runs
// ---------------------------------------------------------------------------

interface VendorState {
  lastRun: string;
  /** `${sourceId}/${relPath}` -> content hash from the previous run. */
  hashes: Record<string, string>;
}

const statePath = () => join(ROOT, VENDOR_DIR, ".state.json");

async function readState(): Promise<VendorState> {
  try {
    return JSON.parse(await readFile(statePath(), "utf-8")) as VendorState;
  } catch {
    return { lastRun: "never", hashes: {} };
  }
}

async function writeState(state: VendorState): Promise<void> {
  await writeFile(statePath(), `${JSON.stringify(state, null, 2)}\n`, "utf-8");
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

async function main(): Promise<number> {
  console.log(
    `${TAG} ${APP.name} stack docs -> ${VENDOR_DIR}/ (${SCRAPED_ON})`,
  );
  const previous = await readState();
  // Start from the previous state, not from zero: a `--source` or `--limit`
  // run refreshes only what it fetched and must leave every other source's
  // hashes (and files) exactly as they were.
  const nextHashes: Record<string, string> = { ...previous.hashes };
  const deltas: {
    source: string;
    added: number;
    changed: number;
    removed: number;
    total: number;
  }[] = [];
  let anyFailed = false;

  for (const source of selected) {
    console.log(`${TAG} ${source.id}: ${source.note}`);
    try {
      const pages =
        source.kind === "site"
          ? await scrapeSite(source)
          : await mirrorGitHub(source);

      const outDir = join(ROOT, VENDOR_DIR, source.id);
      await mkdir(outDir, { recursive: true });

      let added = 0;
      let changed = 0;
      for (const [rel, body] of pages) {
        const key = `${source.id}/${rel}`;
        const hash = sha256(body);
        nextHashes[key] = hash;
        const was = previous.hashes[key];
        if (was === undefined) added++;
        else if (was !== hash) changed++;
        const dest = join(outDir, rel);
        await mkdir(dirname(dest), { recursive: true });
        await writeFile(dest, body, "utf-8");
      }

      // Pages that vanished upstream are removed locally, so the mirror stays
      // an exact image rather than an accretion. Skipped under --limit: a
      // partial fetch is a smoke test, not evidence a page disappeared.
      let removed = 0;
      if (!LIMIT) {
        for (const key of Object.keys(nextHashes)) {
          if (
            !key.startsWith(`${source.id}/`) ||
            pages.has(key.slice(source.id.length + 1))
          )
            continue;
          const rel = key.slice(source.id.length + 1);
          await rm(join(outDir, rel), { force: true });
          delete nextHashes[key];
          removed++;
        }
      }

      // Per-source index, grouped by top folder.
      const groups = new Map<string, { rel: string; title: string }[]>();
      for (const [rel, body] of pages) {
        const title =
          body.match(/^title: "([^"]*)"/m)?.[1] ??
          body.match(/^source_repo: "([^"]*)"/m)?.[1] ??
          rel;
        const group = rel.includes("/") ? rel.split("/")[0] : "(root)";
        if (!groups.has(group)) groups.set(group, []);
        groups.get(group)!.push({ rel, title });
      }
      const index = [
        `# ${source.id} — mirrored documentation`,
        "",
        source.note,
        "",
        `Refreshed ${SCRAPED_ON}; ${pages.size} page(s). Generated by ` +
          `\`bun run docs:fetch\` (scripts/fetch-stack-docs.ts) — do not edit; ` +
          `every file names its upstream source in frontmatter.`,
        "",
        ...(source.kind === "site"
          ? [`Site: ${source.base}`]
          : [
              `Repo: https://github.com/${source.repo} (ref ${source.ref ?? "HEAD"}, ` +
                `paths under ${source.prefix ? `${source.prefix}/` : "the repo root"})`,
            ]),
        "",
      ];
      for (const [group, entries] of [...groups.entries()].sort((a, b) =>
        a[0].localeCompare(b[0]),
      )) {
        index.push(`## ${group}`, "");
        for (const e of entries.sort((a, b) => a.rel.localeCompare(b.rel))) {
          index.push(`- [${e.title}](./${e.rel})`);
        }
        index.push("");
      }
      await writeFile(join(outDir, "INDEX.md"), index.join("\n"), "utf-8");

      deltas.push({
        source: source.id,
        added,
        changed,
        removed,
        total: pages.size,
      });
      console.log(
        `${TAG} ${source.id}: ${pages.size} page(s) — ${added} new, ${changed} changed, ${removed} removed since ${previous.lastRun}.`,
      );
    } catch (err) {
      anyFailed = true;
      console.error(
        `${TAG} ${source.id}: FAILED — ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }

  // Top-level index: what is mirrored, how fresh, how much.
  const topIndex = [
    "# Vendor documentation mirror",
    "",
    "Upstream documentation of the stack this app is built on, mirrored to",
    "plain Markdown by `bun run docs:fetch` (scripts/fetch-stack-docs.ts).",
    "Every file names its upstream source in frontmatter; nothing here is",
    "edited by hand and nothing here is part of the build. The stack-watch",
    "loop that consumes it is documented in `docs/STACK_WATCH.md`.",
    "",
    `Last refresh: ${SCRAPED_ON}.`,
    "",
    "| Source | Pages | Covers |",
    "| ------ | ----- | ------ |",
    // Every mirrored source, not only the ones this run fetched: a `--source`
    // run keeps the others' files, so their rows keep last run's page count.
    ...SOURCES.map((s) => ({
      s,
      total:
        deltas.find((d) => d.source === s.id)?.total ??
        Object.keys(nextHashes).filter((k) => k.startsWith(`${s.id}/`)).length,
    }))
      .filter(({ total }) => total > 0)
      .map(
        ({ s, total }) =>
          `| [${s.id}](./${s.id}/INDEX.md) | ${total} | ${s.note} |`,
      ),
    "",
  ].join("\n");
  await mkdir(join(ROOT, VENDOR_DIR), { recursive: true });
  await writeFile(join(ROOT, VENDOR_DIR, "INDEX.md"), topIndex, "utf-8");

  await writeState({ lastRun: SCRAPED_ON, hashes: nextHashes });

  const anyDelta = deltas.some((d) => d.added + d.changed + d.removed > 0);
  if (!anyDelta && deltas.length > 0) {
    console.log(`${TAG} no upstream changes since ${previous.lastRun}.`);
  } else if (anyDelta) {
    console.log(
      `${TAG} upstream moved — read the pages marked new/changed above against ` +
        `the app before the next release (docs/STACK_WATCH.md).`,
    );
  }
  return anyFailed ? 1 : 0;
}

process.exit(await main());
