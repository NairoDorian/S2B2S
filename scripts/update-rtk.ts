import { execSync } from "node:child_process";

import { APP } from "./app-meta";

/**
 * Reinstalls the `rtk` CLI at the ABSOLUTE LATEST GitHub commit or release tag.
 *
 *   bun run update:rtk [--commit] [--tag <tag>] [--check]
 *
 * Flags:
 *   --commit      Install directly from the latest commit on the default branch (HEAD).
 *   --tag <tag>   Pin an exact tag instead of fetching the latest.
 *   --check       Resolve and print the latest tag, then exit without installing.
 *   --help, -h    Show usage.
 */
const REPO = "rtk-ai/rtk";
const RELEASES_URL = `https://api.github.com/repos/${REPO}/releases`;
const TAGS_URL = `https://api.github.com/repos/${REPO}/tags`;
const GIT_URL = `https://github.com/${REPO}`;
const PER_PAGE = 100;

const API_HEADERS: Record<string, string> = {
  Accept: "application/vnd.github+json",
  "User-Agent": `${APP.slug}-rtk-updater`,
};

interface SemverParts {
  major: number;
  minor: number;
  patch: number;
  /** Pre-release identifiers (e.g. ["rc", "356"]); empty array = stable. */
  prerelease: string[];
  raw: string;
}

/**
 * Parses a GitHub tag like `v0.45.0`, `dev-0.45.1-rc.356`, or `v0.30.1-rc.43`
 * into comparable SemVer parts. Returns null when the tag is not a version.
 */
function parseSemver(tag: string): SemverParts | null {
  const cleaned = tag.replace(/^[vV]/, "").replace(/^dev-/, "");
  const dashParts = cleaned.split("-");
  const version = dashParts[0];
  if (!version) return null;
  const prerelease = dashParts.slice(1).join("-");
  const parts = version.split(".");
  if (parts.length < 3) return null;
  const major = Number(parts[0]);
  const minor = Number(parts[1]);
  const patch = Number(parts[2]);
  if (
    !Number.isInteger(major) ||
    !Number.isInteger(minor) ||
    !Number.isInteger(patch)
  )
    return null;
  return {
    major,
    minor,
    patch,
    prerelease: prerelease ? prerelease.split(".") : [],
    raw: tag,
  };
}

/** Compares two identifiers per SemVer: numeric identifiers compare numerically and sort below alphanumeric ones. */
function comparePrereleaseId(a: string, b: string): number {
  const aNum = /^\d+$/.test(a);
  const bNum = /^\d+$/.test(b);
  if (aNum && bNum) return Number(a) - Number(b);
  if (aNum) return -1; // numeric identifiers always have lower precedence than alphanumeric
  if (bNum) return 1;
  return a < b ? -1 : a > b ? 1 : 0;
}

/** Full SemVer precedence: returns negative when a < b, positive when a > b, 0 when equal. */
function compareSemver(a: SemverParts, b: SemverParts): number {
  if (a.major !== b.major) return a.major - b.major;
  if (a.minor !== b.minor) return a.minor - b.minor;
  if (a.patch !== b.patch) return a.patch - b.patch;
  if (a.prerelease.length === 0 && b.prerelease.length === 0) return 0;
  if (a.prerelease.length === 0) return 1; // stable beats prerelease
  if (b.prerelease.length === 0) return -1;
  const len = Math.min(a.prerelease.length, b.prerelease.length);
  for (let i = 0; i < len; i++) {
    const ai = a.prerelease[i];
    const bi = b.prerelease[i];
    if (ai === undefined || bi === undefined) break; // unreachable given `len` = min length
    const cmp = comparePrereleaseId(ai, bi);
    if (cmp !== 0) return cmp;
  }
  return a.prerelease.length - b.prerelease.length;
}

/**
 * Picks the ABSOLUTE latest tag from a raw list using SemVer precedence
 * (never the API's array order). Falls back to lexicographic when no tag
 * parses as a version.
 */
function absoluteLatestOf(tags: string[]): string | null {
  let best: SemverParts | null = null;
  for (const tag of tags) {
    const parsed = parseSemver(tag);
    if (!parsed) continue;
    if (best === null || compareSemver(parsed, best) > 0) best = parsed;
  }
  if (best) return best.raw;
  let lexBest: string | null = null;
  for (const tag of tags) {
    if (lexBest === null || tag > lexBest) lexBest = tag;
  }
  return lexBest;
}

/**
 * Pages through a GitHub list endpoint (follows the `Link: rel="next"` header)
 * until exhausted.
 */
async function fetchAllPages(
  url: string,
): Promise<Array<Record<string, unknown>>> {
  const res = await fetch(url, { headers: API_HEADERS });
  if (!res.ok) {
    throw new Error(
      `GitHub API request for ${REPO} failed with ${res.status} ${res.statusText} (${url}).`,
    );
  }
  const items = (await res.json()) as Array<Record<string, unknown>>;
  const link = res.headers.get("link");
  const match = link?.match(/<([^>]+)>;\s*rel="next"/);
  const next = match?.[1] ?? null;
  if (!next) return items;
  return [...items, ...(await fetchAllPages(next))];
}

/**
 * Resolves the absolute latest tag. Primary source: the releases API (the
 * same data as the GitHub releases page, drafts excluded). Fallback: the tags
 * API.
 */
async function fetchLatestTag(): Promise<string> {
  try {
    const releases = await fetchAllPages(
      `${RELEASES_URL}?per_page=${PER_PAGE}`,
    );
    const names = releases
      .filter((r) => r.draft !== true)
      .map((r) => String(r.tag_name));
    const latest = absoluteLatestOf(names);
    if (latest) return latest;
    console.warn("No versioned releases found; falling back to the tags API.");
  } catch (err) {
    console.warn(
      `Releases API failed (${err instanceof Error ? err.message : String(err)}); falling back to the tags API.`,
    );
  }

  const tags = await fetchAllPages(`${TAGS_URL}?per_page=${PER_PAGE}`);
  const names = tags.map((t) => String(t.name));
  const latest = absoluteLatestOf(names);
  if (!latest) {
    throw new Error(`No tags found for ${REPO}.`);
  }
  return latest;
}

interface Flags {
  tag?: string;
  commit?: boolean;
  check?: boolean;
  help?: boolean;
}

/** Minimal flag parser. */
function parseFlags(args: string[]): Flags {
  const out: Flags = {};
  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case "--help":
      case "-h":
        out.help = true;
        return out;
      case "--check":
        out.check = true;
        break;
      case "--commit":
        out.commit = true;
        break;
      case "--tag": {
        const value = args[i + 1];
        if (!value) {
          throw new Error("--tag requires a value, e.g. --tag v0.45.0");
        }
        out.tag = value;
        i++;
        break;
      }
    }
  }
  return out;
}

const HELP_TEXT = `
Usage: bun run update:rtk [--commit] [--tag <tag>] [--check]

Reinstalls RTK at the ABSOLUTE LATEST GitHub release tag or commit:
  cargo install --git ${GIT_URL} [--tag <tag>] --force

Flags:
  --commit      Install from the latest git commit on default branch (HEAD).
  --tag <tag>   Pin an exact tag instead of fetching the latest.
  --check       Resolve and print the latest tag, then exit without installing.
  --help, -h    Show this usage.
`;

const flags = parseFlags(process.argv.slice(2));

if (flags.help) {
  console.log(HELP_TEXT);
  process.exit(0);
}

const run = async (): Promise<void> => {
  if (flags.commit) {
    console.log(`Reinstalling RTK from the latest git commit (HEAD)...`);
    execSync(`cargo install --git ${GIT_URL} --force`, {
      stdio: "inherit",
    });
    console.log(`\nRTK updated to latest git commit.`);
    return;
  }

  const tag = flags.tag ?? (await fetchLatestTag());

  if (!/^[\w.-]+$/.test(tag)) {
    throw new Error(`Invalid tag "${tag}" — expected a value like v0.43.0.`);
  }

  if (flags.check) {
    console.log(`Latest RTK tag: ${tag}`);
    return;
  }

  console.log(`Reinstalling RTK at tag ${tag}...`);
  execSync(`cargo install --git ${GIT_URL} --tag ${tag} --force`, {
    stdio: "inherit",
  });
  console.log(`\nRTK updated to ${tag}.`);
};

run().catch((err: unknown) => {
  console.error(
    "Failed to update RTK:",
    err instanceof Error ? err.message : String(err),
  );
  process.exit(1);
});
