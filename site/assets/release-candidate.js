// Release metadata is one snapshot. Never manufacture an asset URL from a tag.
((root) => {
  const repository = "nil-studio/oasis7";
  const releases = `https://github.com/${repository}/releases`;
  const latestApi = `https://api.github.com/repos/${repository}/releases/latest`;
  const filenames = Object.freeze({windows: "oasis7-windows-x64.exe", macos: "oasis7-macos-x64.dmg", linux: "oasis7-linux-x64.deb", checksum: "oasis7-checksums.txt"});
  const fromRelease = (release) => {
    const tag = typeof release?.tag_name === "string" ? release.tag_name.trim() : "";
    const publishedAt = Date.parse(String(release?.published_at || ""));
    if (!tag || tag === "latest" || !Number.isFinite(publishedAt) || release.draft || release.prerelease || !Array.isArray(release.assets)) {
      throw new Error("Incomplete release snapshot");
    }
    const notes = `${releases}/tag/${encodeURIComponent(tag)}`;
    const assets = Object.create(null);
    for (const filename of Object.values(filenames)) {
      const matches = release.assets.filter((asset) => asset.name === filename);
      if (matches.length !== 1) continue;
      const url = matches[0].browser_download_url;
      // Exact trusted repo, candidate tag and filename; no alternate origin/path.
      const expected = `${releases}/download/${encodeURIComponent(tag)}/${filename}`;
      if (url === expected) assets[filename] = url;
    }
    return Object.freeze({tag, publishedAt, notes, assets: Object.freeze(assets)});
  };
  const assetUrl = (candidate, fallback) => {
    if (!candidate) return fallback;
    if (!candidate.assets[filenames.checksum]) return "";
    const filename = String(fallback).split("/").pop();
    return candidate.assets[filename] || "";
  };
  const api = Object.freeze({fromRelease, assetUrl, filenames, latestApi});
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.oasis7Release = api;
})(globalThis);
