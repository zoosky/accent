#!/usr/bin/env bash
# release.sh - Tag and publish a release after the release PR is merged.
#
# Prerequisites:
#   1. The release PR (version bump + CHANGELOG) is merged to main
#   2. You are on main, with a clean tree, at the merge commit
#   3. `cargo login` has stored a crates.io token with publish rights
#      for the `accent` crate
#
# Usage:
#   ./scripts/release.sh v0.1.5
#
# What it does:
#   1. Verifies you are on main, clean, and up to date with origin
#   2. Verifies the [package] version in Cargo.toml matches the tag
#   3. Verifies CHANGELOG.md has a section for the version
#   4. Verifies the tag does not exist, locally or on origin
#   5. Packages the crate with `cargo publish --dry-run` BEFORE the tag
#      exists, so a package error cannot strand a public tag
#   6. Creates an annotated tag on HEAD (the merge commit), pushes it,
#      and runs `cargo publish`
#
# This repo releases with tags only. Do not create a GitHub Release:
# the binaries users install come from AccentCMS/accent, not from here.
set -euo pipefail

if [ $# -ne 1 ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 v0.1.5"
    exit 1
fi

VERSION="$1"
SEMVER="${VERSION#v}"

case "$VERSION" in
    v[0-9]*.[0-9]*.[0-9]*) ;;
    *)
        echo "ERROR: '$VERSION' is not a vMAJOR.MINOR.PATCH tag"
        exit 1
        ;;
esac

# Verify we are on main. Every release of this crate ships from main;
# there are no maintenance lines.
BRANCH=$(git branch --show-current)
if [ "$BRANCH" != "main" ]; then
    echo "ERROR: Must be on main (currently on '$BRANCH')"
    echo "Run: git checkout main && git pull"
    exit 1
fi

# Verify clean working tree
if [ -n "$(git status --porcelain)" ]; then
    echo "ERROR: Working tree is not clean"
    git status --short
    exit 1
fi

# Verify up to date with origin, so the tag lands on the merge commit
# GitHub produced, not on a stale or local-only commit.
git fetch origin main --quiet
LOCAL=$(git rev-parse HEAD)
REMOTE=$(git rev-parse origin/main)
if [ "$LOCAL" != "$REMOTE" ]; then
    echo "ERROR: Local main is not up to date with origin"
    echo "Run: git pull"
    exit 1
fi

# Verify the Cargo.toml version matches the tag
CARGO_VERSION=$(awk '/^\[package\]/{f=1} f && /^version = /{gsub(/version = |"/,""); print; exit}' Cargo.toml)
if [ "$CARGO_VERSION" != "$SEMVER" ]; then
    echo "ERROR: Cargo.toml version ($CARGO_VERSION) does not match tag ($SEMVER)"
    echo "Did you forget to merge the release PR?"
    exit 1
fi

# Verify CHANGELOG has an entry for this version
if ! grep -q "^## \[$SEMVER\]" CHANGELOG.md; then
    echo "ERROR: CHANGELOG.md has no entry for [$SEMVER]"
    exit 1
fi

# Verify the tag does not already exist, locally or on origin
if git rev-parse "$VERSION" >/dev/null 2>&1; then
    echo "ERROR: Tag $VERSION already exists locally"
    exit 1
fi
if git ls-remote --tags origin "refs/tags/$VERSION" | grep -q .; then
    echo "ERROR: Tag $VERSION already exists on origin"
    exit 1
fi

# Package the crate before the tag exists. A manifest or packaging error
# found here costs nothing; found after the push, it strands a public
# tag with no crate behind it.
echo "=== Verifying the package (cargo publish --dry-run) ==="
cargo publish --dry-run

echo ""
echo "=== Releasing $VERSION ==="
echo "  Cargo.toml: $CARGO_VERSION"
echo "  Commit: $(git log --oneline -1)"
echo ""
read -p "Tag, push, and publish to crates.io? [y/N] " -n 1 -r
echo ""

if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 0
fi

git tag -a "$VERSION" -m "Release $VERSION"
echo "Created tag $VERSION"

git push origin "$VERSION"
echo "Pushed tag $VERSION"

cargo publish

echo ""
echo "Done. crates.io serves accent $SEMVER after its index update."
echo "No GitHub Release step exists for this repo: tags only."
