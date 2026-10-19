// A local/CI backstop for accidental publication. Never print matched secrets.
import { execFileSync } from "node:child_process";
import { readFileSync, existsSync, lstatSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const root = fileURLToPath(new URL("../", import.meta.url));
const paths = execFileSync("git", ["ls-files", "--cached", "--others", "--exclude-standard", "-z"], {
  cwd: root,
  encoding: "utf8",
})
  .split("\0")
  .filter(Boolean);
const forbidden =
  /(^|\/)(\.signing\/|mcp-servers\.json(?:\.|$)|mcp-tools-cache\.json(?:\.|$)|launch-items\.json(?:\.|$)|app\.db(?:[.-]|$))|\.(?:key(?:\.bak)?|p12|pfx)$/i;
const findings = new Set();
for (const path of paths) {
  const full = join(root, path);
  if (!existsSync(full) || !lstatSync(full).isFile()) continue;
  if (forbidden.test(path) || (/(^|\/)\.env(?:\.|$)/.test(path) && !/\.example$/.test(path)))
    findings.add(`${path}: private runtime file`);
  const bytes = readFileSync(full);
  if (bytes.includes(0)) continue;
  const text = bytes.toString("utf8");
  if (
    /-----BEGIN (?:RSA |EC |OPENSSH |ENCRYPTED )?PRIVATE KEY-----/.test(text) ||
    /\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{35,}|sk-proj-[A-Za-z0-9_-]{40,})\b/.test(text)
  )
    findings.add(`${path}: credential pattern`);
  for (const token of text.matchAll(/[A-Za-z0-9+/]{80,}={0,2}/g)) {
    if (/untrusted comment:.*(?:private|secret) key/i.test(Buffer.from(token[0], "base64").toString()))
      findings.add(`${path}: encoded private key`);
  }
}
if (findings.size) {
  console.error([...findings].join("\n"));
  process.exitCode = 1;
} else
  console.log(
    `[public-files] checked ${paths.length} candidate files; no known private files or credential patterns found`,
  );
