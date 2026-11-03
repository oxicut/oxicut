#!/usr/bin/env bash
# Print what CHANGELOG.md says about one version (the text under its `## [x.y.z] - date` heading).
# Fails, with a message, when there is no such section or it is empty: the release workflow uses
# this to stop a release whose changelog was not written, and for the release notes.
#
#   tools/changelog-section.sh 0.1.0 [CHANGELOG.md]
set -euo pipefail

version="${1:?usage: changelog-section.sh <version> [file]}"
file="${2:-CHANGELOG.md}"

section=$(awk -v v="$version" '
  BEGIN { gsub(/\./, "\\.", v); pattern = "^## \\[?v?" v "\\]?([ ]|$)" }
  /^## / { if (found) exit; if ($0 ~ pattern) { found = 1; next } }
  found { print }
' "$file")

if [ -z "$(printf '%s' "$section" | tr -d '[:space:]')" ]; then
  echo "$file has no notes for version $version. Add a section '## [$version] - <date>' before releasing." >&2
  exit 1
fi
printf '%s\n' "$section"
