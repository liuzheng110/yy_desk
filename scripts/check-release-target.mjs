// Refuse to overwrite a public version. A failed draft may be retried safely.
const repository = process.env.GITHUB_REPOSITORY;
const tag = process.env.GITHUB_REF_NAME;
const token = process.env.GH_TOKEN;
if (!repository || !tag || !token) throw new Error("Missing release context");
const response = await fetch(`https://api.github.com/repos/${repository}/releases/tags/${encodeURIComponent(tag)}`, {
  headers: {
    Authorization: `Bearer ${token}`,
    Accept: "application/vnd.github+json",
    "User-Agent": "release-validation",
  },
  signal: AbortSignal.timeout(15000),
});
if (response.status !== 404) {
  if (!response.ok) throw new Error(`Cannot inspect release: HTTP ${response.status}`);
  if (!(await response.json()).draft)
    throw new Error("This version is already public. Bump the version instead of replacing a published update.");
}
console.log("[release] target is new or an unpublished draft");
