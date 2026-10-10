import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

test("release: tauri.conf.json version is semver", () => {
  const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
  assert.match(config.version, /^\d+\.\d+\.\d+$/);
});
