#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

SITE_ENTRIES=(
  "${REPO_ROOT}/site/index.html"
  "${REPO_ROOT}/site/en/index.html"
)

REQUIRED_ENTRY_MARKERS=(
  "data-release-tag"
  "data-release-date"
  "data-release-notes-link"
  "data-download-surface"
  "data-download-primary-link"
  "data-download-primary-requirements"
  "data-download-primary-install"
  "data-download-primary-trust"
  "data-download-primary-support"
  "data-download-platform-button=\"windows\""
  "data-download-platform-button=\"macos\""
  "data-download-platform-button=\"linux\""
)

RELEASE_ASSET_URLS=(
  "https://github.com/nil-studio/oasis7/releases/download/2026-06-03_v2/oasis7-windows-x64.exe"
  "https://github.com/nil-studio/oasis7/releases/download/2026-06-03_v2/oasis7-macos-x64.dmg"
  "https://github.com/nil-studio/oasis7/releases/download/2026-06-03_v2/oasis7-linux-x64.deb"
  "https://github.com/nil-studio/oasis7/releases/download/2026-06-03_v2/oasis7-checksums.txt"
)

STATIC_RELEASE_PATTERNS_ZH=(
  "当前版本：<strong data-release-tag>2026-06-03_v2</strong>"
  "<span data-release-date data-release-date-prefix=\"发布时间\">发布时间：2026-06-03</span>"
)

STATIC_RELEASE_PATTERNS_EN=(
  "Current version: <strong data-release-tag>2026-06-03_v2</strong>"
  "<span data-release-date data-release-date-prefix=\"Published\">Published: 2026-06-03</span>"
)

contains_fixed_pattern() {
  local pattern="$1"
  local file_path="$2"
  if command -v rg >/dev/null 2>&1; then
    rg -Fq -- "${pattern}" "${file_path}"
    return $?
  fi
  grep -Fq -- "${pattern}" "${file_path}"
}

for entry in "${SITE_ENTRIES[@]}"; do
  [[ -f "${entry}" ]] || { echo "error: missing site entry: ${entry}" >&2; exit 1; }

  for url in "${RELEASE_ASSET_URLS[@]}"; do
    if ! contains_fixed_pattern "${url}" "${entry}"; then
      echo "error: missing release asset url in ${entry}: ${url}" >&2
      exit 1
    fi
  done

  for marker in "${REQUIRED_ENTRY_MARKERS[@]}"; do
    if ! contains_fixed_pattern "${marker}" "${entry}"; then
      echo "error: missing required download marker in ${entry}: ${marker}" >&2
      exit 1
    fi
  done
done

for pattern in "${STATIC_RELEASE_PATTERNS_ZH[@]}"; do
  if ! contains_fixed_pattern "${pattern}" "${SITE_ENTRIES[0]}"; then
    echo "error: missing accurate Chinese static release fallback: ${pattern}" >&2
    exit 1
  fi
done

for pattern in "${STATIC_RELEASE_PATTERNS_EN[@]}"; do
  if ! contains_fixed_pattern "${pattern}" "${SITE_ENTRIES[1]}"; then
    echo "error: missing accurate English static release fallback: ${pattern}" >&2
    exit 1
  fi
done

python3 - "${SITE_ENTRIES[0]}" "${SITE_ENTRIES[1]}" <<'PY'
import sys
from html.parser import HTMLParser


class DownloadSectionParser(HTMLParser):
    def __init__(self):
        super().__init__()
        self.download_sections = []

    def handle_starttag(self, tag, attrs):
        values = {name.lower(): value or "" for name, value in attrs}
        if tag.lower() == "section" and values.get("id") == "download":
            self.download_sections.append(values)


for path in sys.argv[1:]:
    parser = DownloadSectionParser()
    parser.feed(open(path, encoding="utf-8").read())
    parser.close()
    if len(parser.download_sections) != 1:
        raise SystemExit(f"error: expected one download section in {path}")
    attrs = parser.download_sections[0]
    if "data-reveal" in attrs or "hidden" in attrs or attrs.get("aria-hidden") == "true":
        raise SystemExit(f"error: download section must remain visible without JavaScript in {path}")
PY

if ! contains_fixed_pattern "window.oasis7Release.latestApi" "${REPO_ROOT}/site/assets/app.js"; then
  echo "error: missing latest release api endpoint in site/assets/app.js" >&2
  exit 1
fi

for forbidden in \
  ': "latest";' \
  'https://github.com/nil-studio/oasis7/releases/latest";' \
  'publish time pending'; do
  if contains_fixed_pattern "${forbidden}" "${REPO_ROOT}/site/assets/app.js"; then
    echo "error: release API failure path must preserve static metadata, found fallback: ${forbidden}" >&2
    exit 1
  fi
done

for required in \
  'window.oasis7Release.fromRelease(release)' \
  'releaseRefreshCallbacks.forEach((refresh) => refresh());' \
  'setDownloadLink(linkNode, releaseAssetUrl(next.url));'; do
  if ! contains_fixed_pattern "${required}" "${REPO_ROOT}/site/assets/app.js"; then
    echo "error: missing coherent release/download guard: ${required}" >&2
    exit 1
  fi
done
node "${REPO_ROOT}/scripts/site-release-candidate.test.cjs"
echo "ok: site download entry and candidate-bound assets are present"
