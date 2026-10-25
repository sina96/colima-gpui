#!/bin/bash
# Prints the CHANGELOG.md section for one version, for the GitHub Release notes.
#
#   packaging/release-notes.sh 0.1.0
#
# Fails when CHANGELOG.md has no "## [0.1.0]" section, or it's empty, so a release can't go out
# without notes.
set -euo pipefail

VERSION="${1:?usage: $0 <version, e.g. 0.1.0>}"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

notes="$(awk -v version="$VERSION" '
    # The section starts at "## [VERSION]" and ends at the next "## [" heading or the
    # link references at the bottom.
    index($0, "## [" version "]") == 1 { found = 1; next }
    found && (/^## \[/ || /^\[[^]]+\]: /) { exit }
    found { print }
' "$PROJECT_DIR/CHANGELOG.md" | sed -e '/./,$!d')"

if [[ -z "${notes//[[:space:]]/}" ]]; then
    echo "CHANGELOG.md has no notes for $VERSION (expected a \"## [$VERSION] - YYYY-MM-DD\" section)." >&2
    exit 1
fi
printf '%s\n' "$notes"
