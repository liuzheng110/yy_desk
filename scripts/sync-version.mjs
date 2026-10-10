// 版本号同步脚本：以根 package.json 为单一事实来源，把版本号写到各处。
//
// 应用版本号散落在 4 个文件里，发布前必须一致（release.mjs 与 CI 读的都是
// src-tauri/tauri.conf.json 的 version）。本脚本负责一处改、处处同步。
//
// 用法：
//   pnpm sync-version            # 用根 package.json 的版本覆盖其它文件
//   pnpm sync-version 0.2.0      # 先把根 package.json 改成 0.2.0，再同步到其它文件
//   pnpm sync-version --check    # 只校验各处版本是否一致（不一致则退出码 1，供 CI 用）

import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("../", import.meta.url));
const at = (rel) => path.join(ROOT, rel);

const args = process.argv.slice(2);
const checkOnly = args.includes("--check");
const explicit = args.find((arg) => !arg.startsWith("-"));

const SEMVER = /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/;

/** 读取 JSON 文件的顶层 version（用解析而非正则，稳健）。 */
function jsonVersion(rel) {
  return JSON.parse(readFileSync(at(rel), "utf8")).version;
}

/**
 * 只替换 JSON 里首个 "version" 字段的字面量，其余排版原样保留。
 * 不要整体重新序列化——那样会打乱 Prettier 的排版，导致每次改版本号后
 * 仓库都重新变成"待格式化"状态。
 */
function writeJsonVersion(rel, version) {
  const text = readFileSync(at(rel), "utf8");
  const pattern = /("version"\s*:\s*)"[^"]*"/;
  if (!pattern.test(text)) throw new Error(`${rel} 里找不到 "version" 字段`);
  writeFileSync(at(rel), text.replace(pattern, `$1"${version}"`), "utf8");
}

/** 读 Cargo.toml 中 [package] 段的 version（不碰 [dependencies] 里的同名键）。 */
function cargoVersion(rel) {
  let inPackage = false;
  for (const line of readFileSync(at(rel), "utf8").split(/\r?\n/)) {
    const section = line.match(/^\s*\[([^\]]+)\]/);
    if (section) {
      inPackage = section[1].trim() === "package";
      continue;
    }
    if (inPackage) {
      const match = line.match(/^\s*version\s*=\s*"([^"]+)"/);
      if (match) return match[1];
    }
  }
  throw new Error(`${rel} 的 [package] 段里找不到 version`);
}

function writeCargoVersion(rel, version) {
  let inPackage = false;
  let replaced = false;
  const out = readFileSync(at(rel), "utf8")
    .split(/\r?\n/)
    .map((line) => {
      const section = line.match(/^\s*\[([^\]]+)\]/);
      if (section) {
        inPackage = section[1].trim() === "package";
        return line;
      }
      if (inPackage && !replaced && /^\s*version\s*=/.test(line)) {
        replaced = true;
        return `version = "${version}"`;
      }
      return line;
    })
    .join("\n");
  writeFileSync(at(rel), out, "utf8");
}

// 目标版本：命令行显式指定 > 根 package.json。
const rootPkg = "package.json";
const target = explicit ?? jsonVersion(rootPkg);
if (!SEMVER.test(target)) {
  console.error(`[sync-version] 非法版本号：${target}（应形如 1.2.3 或 1.2.3-rc.1）`);
  process.exit(1);
}

/** 需要保持同步的文件（根 package.json 仅在显式指定版本时才改写）。 */
const targets = [
  { rel: rootPkg, read: jsonVersion, write: writeJsonVersion, skipUnlessExplicit: true },
  { rel: "apps/desktop/package.json", read: jsonVersion, write: writeJsonVersion },
  { rel: "src-tauri/tauri.conf.json", read: jsonVersion, write: writeJsonVersion },
  { rel: "src-tauri/Cargo.toml", read: cargoVersion, write: writeCargoVersion },
];

if (checkOnly) {
  const stale = targets.filter((t) => t.read(t.rel) !== target);
  for (const t of stale) {
    console.error(`[sync-version] ${t.rel}: ${t.read(t.rel)} != ${target}`);
  }
  if (stale.length > 0) {
    console.error(`[sync-version] 版本不一致，请运行 pnpm sync-version ${target}`);
    process.exit(1);
  }
  console.log(`[sync-version] 全部文件版本一致：${target}`);
  process.exit(0);
}

for (const t of targets) {
  if (t.skipUnlessExplicit && !explicit) continue;
  if (t.read(t.rel) === target) continue;
  t.write(t.rel, target);
  console.log(`[sync-version] ${t.rel} → ${target}`);
}

console.log(`[sync-version] 完成，当前版本：${target}`);
