import assert from "node:assert/strict";
import test from "node:test";
import { main } from "./dev-tauri.mjs";

test("dev-tauri: exposes the expected entry point", () => {
  assert.equal(typeof main, "function");
});
