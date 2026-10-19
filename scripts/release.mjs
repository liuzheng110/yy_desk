// MyApp 应用发布脚本（极简版）。
//
// 发布流程（tag 触发 CI 构建并正式发布 Release）：
//   1. pnpm release  <-- 本地校验 + 构建
//   2. git tag vX.Y.Z && git push origin vX.Y.Z
//   3. GitHub Actions 在 windows-2022 上构建 Windows NSIS 安装包（含 latest.json 更新元数据）
//   4. 流水线直接发布正式 Release，客户端据此自动更新
//
// 用法：node scripts/release.mjs [--skip-build] [--dry-run]

import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const args = process.argv.slice(2);
const skipBuild = args.includes("--skip-build");
const dryRun = args.includes("--dry-run");

function fail(message) {
  console.error(message);
  process.exit(1);
}

function run(command, commandArgs, options = {}) {
  // Only fixed script names are passed here. Windows needs cmd for pnpm.cmd.
  const windows = process.platform === "win32";
  const result = spawnSync(
    windows ? "cmd.exe" : command,
    windows ? ["/d", "/s", "/c", [command, ...commandArgs].join(" ")] : commandArgs,
    {
      cwd: process.cwd(),
      env: process.env,
      encoding: "utf8",
      stdio: options.stdio ?? "pipe",
    },
  );
  if (result.status !== 0) {
    fail(`${command} ${commandArgs.join(" ")} failed:\n${result.error?.message ?? (result.stderr || "").trim()}`);
  }
  return result;
}

const tauriConfig = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const version = tauriConfig.version;
const gitTag = `v${version}`;

console.log(`[release] version: ${version}`);
console.log(`[release] git tag: ${gitTag}`);

if (!dryRun) {
  console.log("[release] validating application, public files and regression tests…");
  run("pnpm", ["validate"], { stdio: "inherit" });
}
if (!dryRun && !skipBuild) {
  console.log("[release] running tauri build…");
  run("pnpm", ["tauri", "build"], { stdio: "inherit" });
}

console.log(`[release] next steps:
  1. review git diff and stage the intended source changes, then commit
  2. git tag ${gitTag}
  3. git push origin ${gitTag}
  4. wait for checks, signed draft build and asset verification; CI then publishes automatically
`);
