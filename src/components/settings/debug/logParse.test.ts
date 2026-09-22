import { describe, expect, test } from "bun:test";
import { parseLogRecords, tagFromLevel } from "./logParse";

describe("log record header parse", () => {
  test("captures date, time, target, level, and message", () => {
    const line =
      "[2026-09-22][14:49:47][app_lib][INFO] Vulkan layer policy: ~implicit~";
    const [rec] = parseLogRecords(line);
    expect(rec).toMatchObject({
      tag: "INF",
      time: "14:49:47",
      target: "app_lib",
      message: "Vulkan layer policy: ~implicit~",
    });
  });

  test("maps levels to viewer tags", () => {
    expect(tagFromLevel("DEBUG")).toBe("DBG");
    expect(tagFromLevel("TRACE")).toBe("TRC");
    expect(tagFromLevel("WARN")).toBe("WRN");
    expect(tagFromLevel("ERROR")).toBe("ERR");
    expect(tagFromLevel("INFO")).toBe("INF");
  });

  test("message is never the literal word undefined", () => {
    const line =
      "[2026-09-22][14:49:47][rusqlite_migration][DEBUG] Running: CREATE";
    const [rec] = parseLogRecords(line);
    expect(rec.message).toBe("Running: CREATE");
    expect(rec.message).not.toBe("undefined");
  });

  test("nested module targets keep their :: separators", () => {
    const line =
      "[2026-09-22][14:49:50][app_lib::settings][TRACE] Loaded settings: { }";
    const [rec] = parseLogRecords(line);
    expect(rec).toMatchObject({
      tag: "TRC",
      time: "14:49:50",
      target: "app_lib::settings",
      message: "Loaded settings: { }",
    });
  });

  test("continuation lines fold into the previous record", () => {
    const text = [
      "[2026-09-22][14:49:47][rusqlite_migration][DEBUG] Running: CREATE TABLE (",
      "            id INTEGER PRIMARY KEY AUTOINCREMENT,",
      "        );",
    ].join("\n");
    const records = parseLogRecords(text);
    expect(records).toHaveLength(1);
    expect(records[0].message).toContain("id INTEGER PRIMARY KEY");
    expect(records[0].message).toContain(");");
    expect(records[0].tag).toBe("DBG");
    expect(records[0].time).toBe("14:49:47");
    expect(records[0].message).not.toContain("undefined");
  });

  test("parses a real session file slice without undefined messages", () => {
    const text = [
      "[2026-09-22][14:49:47][app_lib][INFO] app starting on windows",
      "[2026-09-22][14:49:47][rusqlite_migration][DEBUG] Running: CREATE TABLE IF NOT EXISTS transcription_history (",
      "            id INTEGER PRIMARY KEY AUTOINCREMENT,",
      "            file_name TEXT NOT NULL",
      "        );",
      "[2026-09-22][14:49:50][transcribe_cpp][INFO] load_backend: loaded CUDA backend",
    ].join("\n");
    const records = parseLogRecords(text);
    expect(records).toHaveLength(3);
    for (const rec of records) {
      expect(rec.message).not.toBe("undefined");
      expect(rec.message.length).toBeGreaterThan(0);
    }
    expect(records[1].message).toContain("file_name TEXT NOT NULL");
    expect(records[1].target).toBe("rusqlite_migration");
    expect(records[2].target).toBe("transcribe_cpp");
  });
});
