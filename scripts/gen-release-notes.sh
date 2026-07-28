#!/bin/sh
# Generate release notes from git commits since the last tag.
# Usage: gen-release-notes.sh <version>
set -eu

version="${1:?usage: gen-release-notes.sh <version>}"
repo_url="https://github.com/PaRr0tBoY/herdr"

# Find the previous release tag (first-parent traversal)
prev_tag="$(git describe --tags --first-parent --match 'v[0-9]*' --abbrev=0 HEAD^ 2>/dev/null || true)"

if [ -z "$prev_tag" ]; then
    range="HEAD"
    prev_label="the beginning"
else
    range="${prev_tag}..HEAD"
    prev_label="${prev_tag}"
fi

echo "# Hive v${version}"
echo ""

if [ -z "$prev_tag" ]; then
    echo "First release."
else
    echo "Changes since ${prev_tag}:"
    echo ""
    git log --format="* %s" "$range" 2>/dev/null \
        | grep -v '^* $' \
        | head -200 || echo "(no changes found)"
fi

echo ""
echo "---"
if [ -n "$prev_tag" ]; then
    echo "Compare: ${repo_url}/compare/${prev_tag}...v${version}"
else
    echo "Release: ${repo_url}/releases/tag/v${version}"
fi
