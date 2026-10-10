import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import { realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";

const entryPath = fileURLToPath(import.meta.url);

function run(executable, args) {
  const child = spawn(executable, args, { stdio: "inherit", env: process.env });
  const handlers = new Map();
  for (const signal of ["SIGINT", "SIGTERM"]) {
    const handler = () => child.kill(signal);
    handlers.set(signal, handler);
    process.on(signal, handler);
  }
  child.on("error", (error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
  child.on("close", (code, signal) => {
    for (const [name, handler] of handlers) process.removeListener(name, handler);
    if (signal) process.kill(process.pid, signal);
    else process.exitCode = code ?? 1;
  });
}

export function main(args = process.argv.slice(2)) {
  const tauri = createRequire(import.meta.url).resolve("@tauri-apps/cli/tauri.js");
  run(process.execPath, [tauri, "dev", ...args]);
}

if (process.argv[1] && realpathSync.native(process.argv[1]) === realpathSync.native(entryPath)) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
