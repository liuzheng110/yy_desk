// MyApp 应用发布脚本（极简版）。
//
// 发布流程（与 dbx 相同：tag 触发 CI 构建草稿 Release）：
//   1. pnpm release  <-- 本地校验 + 构建
//   2. git tag vX.Y.Z && git push origin vX.Y.Z
//   3. GitHub Actions 在三个平台构建并上传安装包（含 latest.json 更新元数据）
//   4. 在 GitHub Releases 页面检查/发布草稿
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
  const result = spawnSync(command, commandArgs, {
    cwd: process.cwd(),
    env: process.env,
    encoding: "utf8",
    stdio: options.stdio ?? "pipe",
  });
  if (result.status !== 0) {
    fail(`${command} ${commandArgs.join(" ")} failed:\n${(result.stderr || "").trim()}`);
  }
  return result;
}

const tauriConfig = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const version = tauriConfig.version;
const gitTag = `v${version}`;

console.log(`[release] version: ${version}`);
console.log(`[release] git tag: ${gitTag}`);

if (!dryRun && !skipBuild) {
  console.log("[release] running pnpm build:checked (typecheck + vite build)…");
  run("pnpm", ["build:checked"]);
  console.log("[release] running tauri build…");
  run("pnpm", ["tauri", "build"]);
}

console.log(`[release] next steps:
  1. git add -A && git commit -m "release: ${gitTag}"
  2. git tag ${gitTag}
  3. git push origin ${gitTag}
  4. wait for the Release workflow, then publish the draft on GitHub Releases
`);
