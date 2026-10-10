import { spawn } from "node:child_process";
import { performance } from "node:perf_hooks";
import { delimiter, join } from "node:path";

const tasks = [
  {
    name: "typecheck",
    command: "vue-tsc",
    args: ["--noEmit", "--project", "apps/desktop/tsconfig.json"],
  },
  {
    name: "rust-check",
    command: "cargo",
    args: ["check", "--workspace"],
  },
];

function runTask(task) {
  const startedAt = performance.now();
  // 经 shell 执行：Windows 上 node_modules/.bin 里是 .cmd 垫片，
  // Node 18+ 出于安全不再允许无 shell 直接 spawn .bat/.cmd。
  // 传整条命令串（不用 args 数组），避免 Node 对 shell+args 的弃用告警。
  const child = spawn([task.command, ...task.args].join(" "), {
    shell: true,
    cwd: process.cwd(),
    env: {
      ...process.env,
      PATH: [join(process.cwd(), "node_modules", ".bin"), process.env.PATH].filter(Boolean).join(delimiter),
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const stdout = [];
  const stderr = [];

  child.stdout.on("data", (chunk) => stdout.push(chunk));
  child.stderr.on("data", (chunk) => stderr.push(chunk));

  return new Promise((resolve) => {
    child.on("error", (error) => {
      resolve({
        ...task,
        code: 1,
        durationMs: performance.now() - startedAt,
        output: "",
        errorOutput: error.stack ?? String(error),
      });
    });
    child.on("close", (code) => {
      resolve({
        ...task,
        code,
        durationMs: performance.now() - startedAt,
        output: Buffer.concat(stdout).toString(),
        errorOutput: Buffer.concat(stderr).toString(),
      });
    });
  });
}

const results = await Promise.all(tasks.map(runTask));

for (const result of results) {
  const seconds = (result.durationMs / 1000).toFixed(2);
  console.log(`${(result.code === 0 ? "ok" : "failed").padEnd(6)} ${result.name.padEnd(12)} ${seconds}s`);
}

const failures = results.filter((result) => result.code !== 0);
for (const failure of failures) {
  console.error(`\n${failure.name} failed:`);
  const lines = (failure.output || failure.errorOutput || "").trimEnd().split(/\r?\n/);
  console.error(lines.slice(-80).join("\n"));
}

if (failures.length > 0) {
  process.exit(1);
}
