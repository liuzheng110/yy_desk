import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash, createPublicKey, verify } from "node:crypto";
import { validateAppConfig } from "./check-app-config.mjs";

export function verifySignedPackage(bytes, encodedSignature, encodedPublicKey) {
  const publicLines = Buffer.from(encodedPublicKey, "base64").toString().trim().split(/\r?\n/);
  const publicKey = Buffer.from(publicLines[1] ?? "", "base64");
  const lines = Buffer.from(encodedSignature, "base64").toString().trim().split(/\r?\n/);
  const signature = Buffer.from(lines[1] ?? "", "base64");
  if (
    publicKey.length !== 42 ||
    signature.length !== 74 ||
    !signature.subarray(2, 10).equals(publicKey.subarray(2, 10)) ||
    !lines[2]?.startsWith("trusted comment: ")
  )
    throw new Error("Invalid signature or signing key mismatch");
  const algorithm = signature.subarray(0, 2).toString();
  if (algorithm !== "ED" && algorithm !== "Ed") throw new Error("Unsupported minisign algorithm");
  const key = createPublicKey({
    key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), publicKey.subarray(10)]),
    format: "der",
    type: "spki",
  });
  const message = algorithm === "ED" ? createHash("blake2b512").update(bytes).digest() : bytes;
  const comment = Buffer.from(lines[2].slice("trusted comment: ".length));
  if (
    !verify(null, message, key, signature.subarray(10)) ||
    !verify(null, Buffer.concat([signature.subarray(10), comment]), key, Buffer.from(lines[3] ?? "", "base64"))
  )
    throw new Error("Update package signature verification failed");
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const config = JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"));
    const { repository } = validateAppConfig(config);
    const dir = process.argv[2];
    if (!dir) throw new Error("Usage: node scripts/verify-release.mjs <downloaded-assets-directory>");
    const manifest = JSON.parse(readFileSync(join(dir, "latest.json"), "utf8"));
    if (manifest.version !== config.version) throw new Error("Release manifest version mismatch");
    const target = manifest.platforms?.["windows-x86_64-nsis"] ?? manifest.platforms?.["windows-x86_64"];
    if (!target) throw new Error("Missing Windows x64 updater target");
    for (const asset of Object.values(manifest.platforms)) {
      const prefix = `https://github.com/${repository}/releases/download/v${config.version}/`;
      if (!asset.url?.startsWith(prefix)) throw new Error("Release asset URL points outside this version/repository");
      const filename = decodeURIComponent(asset.url.slice(prefix.length));
      if (!filename.endsWith(".exe") || /[/\\?#]/.test(filename)) throw new Error("Invalid installer asset name");
      const bytes = readFileSync(join(dir, filename));
      if (bytes.length < 2 || bytes[0] !== 0x4d || bytes[1] !== 0x5a)
        throw new Error("Installer is not a Windows executable");
      verifySignedPackage(bytes, asset.signature, config.plugins.updater.pubkey);
      if (readFileSync(join(dir, `${filename}.sig`), "utf8").trim() !== asset.signature.trim())
        throw new Error("Manifest signature differs from published .sig asset");
    }
    console.log(`[release] v${manifest.version}: package signatures and update metadata verified`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
