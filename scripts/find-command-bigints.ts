import fs from "fs";
import path from "path";

const srcDir = path.resolve(__dirname, "../src-tauri/src");

function getFiles(dir: string): string[] {
  let results: string[] = [];
  const list = fs.readdirSync(dir);
  list.forEach((file) => {
    const filePath = path.join(dir, file);
    const stat = fs.statSync(filePath);
    if (stat && stat.isDirectory()) {
      results = results.concat(getFiles(filePath));
    } else if (file.endsWith(".rs")) {
      results.push(filePath);
    }
  });
  return results;
}

const bigintTypes = ["i64", "u64", "usize", "isize", "i128", "u128"];

function analyze() {
  const files = getFiles(srcDir);
  for (const file of files) {
    const content = fs.readFileSync(file, "utf8");
    if (
      !content.includes("tauri::command") &&
      !content.includes("specta::specta")
    )
      continue;

    const lines = content.split("\n");
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i].trim();
      if (
        line.includes("fn ") &&
        (lines[i - 1]?.includes("tauri::command") ||
          lines[i - 1]?.includes("specta") ||
          lines[i - 2]?.includes("tauri::command"))
      ) {
        // We found a tauri command function signature!
        // Read lines until the closing parenthesis
        let signature = line;
        let j = i + 1;
        while (!signature.includes("{") && j < lines.length) {
          signature += " " + lines[j].trim();
          j++;
        }

        // Find if signature has bigint types in arguments or return type
        for (const bt of bigintTypes) {
          const regex = new RegExp(`\\b${bt}\\b`);
          if (regex.test(signature)) {
            console.log(
              `File: ${path.relative(srcDir, file)} | Command: ${line}`,
            );
            console.log(`  Signature: ${signature}`);
          }
        }
      }
    }
  }
}

analyze();
