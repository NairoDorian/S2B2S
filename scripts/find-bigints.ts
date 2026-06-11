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
    if (!content.includes("Type") && !content.includes("specta")) continue;

    const lines = content.split("\n");
    let insideStructOrEnum = false;
    let structName = "";
    let structLines: string[] = [];

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i].trim();

      if (line.includes("struct ") || line.includes("enum ")) {
        const match = line.match(/(?:struct|enum)\s+(\w+)/);
        if (match) {
          structName = match[1];
          insideStructOrEnum = true;
          structLines = [];
        }
      }

      if (insideStructOrEnum) {
        structLines.push(`${i + 1}: ${lines[i]}`);
        if (line.startsWith("}") || line.endsWith("}")) {
          // Finished parsing struct/enum, let's analyze it
          const joined = structLines.join("\n");
          const isSpectaType =
            joined.includes("Type") &&
            (joined.includes("derive") || joined.includes("specta"));
          if (isSpectaType) {
            // Find any bigint fields that don't have a specta(type = ...) override
            for (const lineOfStruct of structLines) {
              const trimmedLine = lineOfStruct.trim();
              // Look for field declarations like: pub field_name: type,
              const fieldMatch = trimmedLine.match(
                /(?:pub\s+)?(\w+)\s*:\s*(.+),?/,
              );
              if (fieldMatch) {
                const fieldType = fieldMatch[2].trim().replace(",", "");
                // Check if the type contains any of the bigint types
                const containsBigint = bigintTypes.some((bt) => {
                  const regex = new RegExp(`\\b${bt}\\b`);
                  return regex.test(fieldType);
                });
                if (containsBigint) {
                  // Check if previous lines have specta override
                  const prevLineIdx = structLines.indexOf(lineOfStruct) - 1;
                  const hasOverride =
                    prevLineIdx >= 0 &&
                    structLines[prevLineIdx].includes("specta(type =");
                  if (!hasOverride) {
                    console.log(
                      `File: ${path.relative(srcDir, file)} | Struct: ${structName}`,
                    );
                    console.log(`  Line ${lineOfStruct}`);
                  }
                }
              }
            }
          }
          insideStructOrEnum = false;
        }
      }
    }
  }
}

analyze();
