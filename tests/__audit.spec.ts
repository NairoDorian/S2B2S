// TEMPORARY audit: collect Solid 2 dev warnings and report unique counts per
// component. Delete when the reactivity cleanup is done.
import { test } from "./harness/fixtures";

const SECTIONS = [
  "General",
  "History",
  "Recall",
  "Statistics",
  "Models",
  "Multi STT",
  "Transcribe Files",
  "Live Mode",
  "Live FFT",
  "Overlay",
  "Local LLM",
  "Advanced",
  "Debug",
  "Help",
  "About",
];

test("collect solid dev warnings across all pages", async ({ page }) => {
  const warnings: string[] = [];
  page.on("console", (msg) => {
    const text = msg.text();
    if (text.includes("STRICT_READ_UNTRACKED") || text.includes("SERVER_WRITE"))
      warnings.push(text);
  });

  await page.goto("/");
  for (const label of SECTIONS) {
    await page
      .getByRole("navigation")
      .getByRole("button", { name: label, exact: true })
      .click();
    await page.waitForTimeout(300);
  }

  const counts = new Map<string, number>();
  for (const w of warnings) {
    const m = w.match(/in <([^>]+)>/);
    const name = m ? m[1] : "(unknown)";
    counts.set(name, (counts.get(name) ?? 0) + 1);
  }
  const sorted = [...counts.entries()].sort((a, b) => b[1] - a[1]);
  console.log(
    `\n=== ${warnings.length} warnings, ${sorted.length} components ===`,
  );
  for (const [name, n] of sorted)
    console.log(`${String(n).padStart(5)}  ${name}`);
});
