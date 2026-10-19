import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

export function validateAppConfig(config, { repository, tag, version } = {}) {
  const fail = (message) => {
    throw new Error(message);
  };
  if (!/^[A-Za-z0-9]+(?:[.-][A-Za-z0-9]+)+$/.test(config.identifier ?? "")) fail("Invalid application identifier");
  if (
    !config.productName?.trim() ||
    [...config.productName].some((c) => c.charCodeAt(0) < 32) ||
    /[<>:"/\\|?*&]/.test(config.productName)
  )
    fail("Invalid productName");
  const schemes = config.plugins?.["deep-link"]?.desktop?.schemes;
  if (!Array.isArray(schemes) || schemes.length !== 1 || !/^[a-z][a-z0-9+.-]*$/.test(schemes[0]))
    fail("Configure one valid deep-link scheme");
  const updater = config.plugins?.updater;
  const endpoints = updater?.endpoints;
  if (!Array.isArray(endpoints) || endpoints.length !== 1) fail("Configure one canonical update endpoint");
  const match = endpoints[0].match(
    /^https:\/\/github\.com\/([A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+)\/releases\/latest\/download\/latest\.json$/,
  );
  if (!match) fail("Expected a GitHub latest.json endpoint");
  const key = Buffer.from(updater.pubkey ?? "", "base64")
    .toString("utf8")
    .trim()
    .split(/\r?\n/);
  const decoded = Buffer.from(key[1] ?? "", "base64");
  if (!key[0]?.includes("public key") || decoded.length !== 42 || decoded.subarray(0, 2).toString() !== "Ed")
    fail("Updater requires a minisign public key, never a private key");
  if (version && config.version !== version) fail("Application version differs from package.json");
  if (tag && (tag !== `v${config.version}` || !/^\d+\.\d+\.\d+$/.test(config.version)))
    fail("Release tag must match the stable application version");
  if (repository && repository.toLowerCase() !== match[1].toLowerCase())
    fail("Release repository differs from updater endpoint; configure this derived app before publishing");
  return { repository: match[1], productName: config.productName };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = new URL("../", import.meta.url);
  try {
    const config = JSON.parse(readFileSync(new URL("src-tauri/tauri.conf.json", root), "utf8"));
    const pkg = JSON.parse(readFileSync(new URL("package.json", root), "utf8"));
    const release = process.argv.includes("--release");
    if (release && (!process.env.GITHUB_REPOSITORY || !process.env.GITHUB_REF_NAME))
      throw new Error("Release validation requires GitHub repository and tag context");
    const result = validateAppConfig(config, {
      version: pkg.version,
      repository: release ? process.env.GITHUB_REPOSITORY : undefined,
      tag: release ? process.env.GITHUB_REF_NAME : undefined,
    });
    console.log(`[app-config] ${result.productName}: ${result.repository}`);
    if (process.env.GITHUB_OUTPUT) {
      const { appendFileSync } = await import("node:fs");
      appendFileSync(process.env.GITHUB_OUTPUT, `product_name=${result.productName}\n`);
    }
  } catch (error) {
    console.error(`[app-config] ${error.message}`);
    process.exitCode = 1;
  }
}
