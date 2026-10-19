import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import ts from "typescript";

const require = createRequire(new URL("../../apps/desktop/package.json", import.meta.url));

// Execute the actual TS module with transport boundaries stubbed, using the real Vue/Pinia runtime.
export function loadFrontend(relative, overrides = {}, globals = {}) {
  const source = readFileSync(new URL(`../../apps/desktop/src/${relative}`, import.meta.url), "utf8");
  const code = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const exports = {};
  const context = vm.createContext({
    exports,
    console: { error() {}, warn() {} },
    crypto: globalThis.crypto,
    window: { __TAURI_INTERNALS__: {}, setTimeout: () => 1, clearTimeout() {} },
    require: (id) => (Object.hasOwn(overrides, id) ? overrides[id] : require(id)),
    ...globals,
  });
  new vm.Script(code, { filename: relative }).runInContext(context);
  return exports;
}

export function newPinia() {
  const { createPinia, setActivePinia } = require("pinia");
  setActivePinia(createPinia());
}
