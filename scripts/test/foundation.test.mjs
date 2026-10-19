import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { generateKeyPairSync, sign, createHash } from "node:crypto";
import { validateAppConfig } from "../check-app-config.mjs";
import { verifySignedPackage } from "../verify-release.mjs";
import { loadFrontend, newPinia } from "./frontend-loader.mjs";

const config = JSON.parse(readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8"));
const cached = {
  cache_id: "cache-1",
  version: "0.2.0",
  portable_mode: false,
  release_url: "https://example.test/release",
  release_notes: "",
  downloaded_at: 1,
};
function updater(overrides = {}, eventOverrides = {}) {
  newPinia();
  let checks = 0;
  let downloads = 0;
  const backend = {
    getDownloadedUpdate: async () => cached,
    checkForUpdates: async () => {
      checks++;
      return { update_available: true, latest_version: "0.2.0" };
    },
    downloadUpdate: async () => {
      downloads++;
      return cached;
    },
    installDownloadedUpdate: async () => {},
    ...overrides,
  };
  const mod = loadFrontend("stores/updater.ts", {
    "@/lib/backend": { backend, UPDATE_DOWNLOAD_PROGRESS_EVENT: "progress" },
    "@/lib/identity": { APP_NAME: "TestApp" },
    "./app": { useAppStore: () => ({ version: "0.1.0", init: async () => {} }) },
    "@tauri-apps/api/event": { listen: async () => () => {}, ...eventOverrides },
  });
  return { store: mod.useUpdaterStore(), counts: () => ({ checks, downloads }) };
}

test("release refuses mismatched repository, version and tag", () => {
  assert.equal(validateAppConfig(config).productName, config.productName);
  assert.throws(() => validateAppConfig(config, { repository: "another/app" }), /repository/);
  assert.throws(() => validateAppConfig(config, { tag: "v99.0.0" }), /tag/);
  assert.throws(() => validateAppConfig(config, { version: "99.0.0" }), /version/);
});

test("release rejects invalid public key and non-HTTPS update source", () => {
  const copy = structuredClone(config);
  copy.plugins.updater.pubkey = "invalid";
  assert.throws(() => validateAppConfig(copy), /public key/);
  copy.plugins.updater.endpoints[0] = "http://example.test/latest.json";
  assert.throws(() => validateAppConfig(copy), /GitHub/);
});

test("signature verification rejects altered package, comment and wrong key", () => {
  // Ephemeral test key only; no production secret is read or stored.
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const keyId = Buffer.from("0102030405060708", "hex");
  const encodedKey = Buffer.from(
    `untrusted comment: minisign public key\n${Buffer.concat([Buffer.from("Ed"), keyId, publicKey.export({ format: "der", type: "spki" }).subarray(-32)]).toString("base64")}\n`,
  ).toString("base64");
  const bytes = Buffer.from("test installer");
  const signature = sign(null, createHash("blake2b512").update(bytes).digest(), privateKey);
  const comment = "timestamp:1";
  const lines = [
    "untrusted comment: test signature",
    Buffer.concat([Buffer.from("ED"), keyId, signature]).toString("base64"),
    `trusted comment: ${comment}`,
    sign(null, Buffer.concat([signature, Buffer.from(comment)]), privateKey).toString("base64"),
  ];
  const encoded = Buffer.from(lines.join("\n")).toString("base64");
  verifySignedPackage(bytes, encoded, encodedKey);
  assert.throws(() => verifySignedPackage(Buffer.from("tampered"), encoded, encodedKey), /verification/);
  const changed = [...lines];
  changed[2] += "tampered";
  assert.throws(
    () => verifySignedPackage(bytes, Buffer.from(changed.join("\n")).toString("base64"), encodedKey),
    /verification/,
  );
  assert.throws(() => verifySignedPackage(bytes, encoded, config.plugins.updater.pubkey), /mismatch/);
});

test("checking again preserves a downloaded package and does not redownload", async () => {
  const { store, counts } = updater();
  await store.start();
  assert.equal(store.state, "downloaded");
  await store.check(true);
  assert.equal(store.state, "downloaded");
  assert.equal(store.cached.cache_id, cached.cache_id);
  await store.onClick();
  assert.deepEqual(counts(), { checks: 0, downloads: 0 });
  store.stop();
});

test("failed installation with an evicted cache permits a fresh download", async () => {
  let reads = 0;
  const { store } = updater({
    getDownloadedUpdate: async () => (++reads === 1 ? cached : null),
    installDownloadedUpdate: async () => {
      throw new Error("cache corrupt");
    },
  });
  await store.start();
  await store.install();
  assert.equal(store.cached, null);
  assert.equal(store.state, "available");
  assert.equal(store.installing, false);
  store.stop();
});

test("duplicate install requests are ignored", async () => {
  let finish;
  let calls = 0;
  const { store } = updater({
    installDownloadedUpdate: () => {
      calls++;
      return new Promise((resolve) => {
        finish = resolve;
      });
    },
  });
  await store.start();
  const pending = store.install();
  await store.install();
  await store.check(true);
  assert.equal(calls, 1);
  assert.equal(store.state, "downloaded");
  finish();
  await pending;
  store.stop();
});

test("stopping during listener setup removes the late listener", async () => {
  let attach;
  let removed = 0;
  let signal;
  const listening = new Promise((resolve) => {
    signal = resolve;
  });
  const { store } = updater(
    {},
    {
      listen: () =>
        new Promise((resolve) => {
          attach = resolve;
          signal();
        }),
    },
  );
  const pending = store.start();
  await listening;
  store.stop();
  attach(() => {
    removed++;
  });
  await pending;
  assert.equal(removed, 1);
  assert.equal(store.initializing, false);
});

test("startup is not marked ready when the backend handshake fails", async () => {
  const mod = loadFrontend("lib/startup.ts", {
    "./backend": {
      backend: {
        markFrontendReady: async () => {
          throw new Error("not ready");
        },
      },
    },
    "@tauri-apps/api/event": { listen: async () => () => {} },
  });
  await assert.rejects(mod.markFrontendReady(), /not ready/);
  assert.equal(mod.isFrontendReady(), false);
});

test("partial startup subscription is rolled back", async () => {
  let calls = 0;
  let removed = 0;
  const mod = loadFrontend("lib/startup.ts", {
    "./backend": { backend: {} },
    "@tauri-apps/api/event": {
      listen: async () => {
        if (++calls === 2) throw new Error("listen failed");
        return () => {
          removed++;
        };
      },
    },
  });
  await assert.rejects(mod.connectStartupEvents(), /listen failed/);
  assert.equal(removed, 1);
});
