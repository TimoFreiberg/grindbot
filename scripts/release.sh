#!/usr/bin/env bash
#
# Release script for grindbot.
#
# Usage:
#   scripts/release.sh [version]
#
# Without an argument, reads the current version from Cargo.toml.
# With a version argument (e.g. "0.2.0"), bumps Cargo.toml first.
#
# Steps:
#   1. Ensure working copy is clean (no uncommitted changes).
#   2. Ensure version in Cargo.toml matches the tag.
#   3. Commit the version bump (if any).
#   4. Create and push the version tag.
#   5. CI builds and publishes the GitHub Release automatically.
#
# Prerequisites:
#   - jj (Jujutsu)
#   - The repo must have a remote configured for push.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

die() {
    echo "error: $*" >&2
    exit 1
}

get_cargo_version() {
    # Extract the version field from the [package] section of Cargo.toml.
    awk '/^\[package\]/{f=1} f && /^version[[:space:]]*=/{gsub(/[version"=[:space:]]/,""); print; exit}' Cargo.toml
}

set_cargo_version() {
    local new_version="$1"
    # Use a temp sed file to avoid -i portability issues.
    sed -E "s/^version = \".*\"/version = \"${new_version}\"/" Cargo.toml > Cargo.toml.tmp
    mv Cargo.toml.tmp Cargo.toml
}

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

REQUESTED_VERSION="${1:-}"

CURRENT_VERSION="$(get_cargo_version)"
echo "Current Cargo.toml version: ${CURRENT_VERSION}"

if [[ -n "$REQUESTED_VERSION" ]]; then
    TARGET_VERSION="$REQUESTED_VERSION"
else
    TARGET_VERSION="$CURRENT_VERSION"
fi

echo "Releasing version: ${TARGET_VERSION}"

TAG="v${TARGET_VERSION}"

# ---------------------------------------------------------------------------
# 1. Pre-flight checks
# ---------------------------------------------------------------------------

# Ensure we're in a jj repo.
jj log -r '@' --no-graph -T '' >/dev/null 2>&1 || die "not in a jj repo"

# Check for uncommitted changes in the working copy.
# (We allow changes if we're about to bump the version — but if no bump is
# needed, the tree should be clean.)
if [[ "$TARGET_VERSION" != "$CURRENT_VERSION" ]]; then
    echo "Bumping Cargo.toml version ${CURRENT_VERSION} → ${TARGET_VERSION}"
    set_cargo_version "$TARGET_VERSION"
    # Verify it took.
    ACTUAL="$(get_cargo_version)"
    [[ "$ACTUAL" == "$TARGET_VERSION" ]] || die "version bump failed: expected ${TARGET_VERSION}, got ${ACTUAL}"
else
    # No version change — make sure there's nothing uncommitted.
    # jj diff exits 0 with empty output when there are no changes.
    DIFF_OUTPUT="$(jj diff --stat 2>/dev/null || true)"
    [[ -z "$DIFF_OUTPUT" ]] || die "working copy has uncommitted changes; commit or discard them first:\n${DIFF_OUTPUT}"
fi

# ---------------------------------------------------------------------------
# 2. Commit the version bump (if any)
# ---------------------------------------------------------------------------

if [[ "$TARGET_VERSION" != "$CURRENT_VERSION" ]]; then
    jj commit Cargo.toml -m "Bump version to ${TARGET_VERSION}"
    echo "Committed version bump."
fi

# ---------------------------------------------------------------------------
# 3. Create and push the tag
# ---------------------------------------------------------------------------

# Check if the tag already exists locally.
EXISTING_TAG="$(jj tag list 2>/dev/null | grep "^${TAG}$" || true)"
if [[ -n "$EXISTING_TAG" ]]; then
    die "tag ${TAG} already exists locally. Delete it first with: jj tag delete ${TAG}"
fi

echo "Creating tag ${TAG}…"

# Create the tag on the current commit (defaults to @).
jj tag set "${TAG}" || die "failed to create tag ${TAG}"

# Push the tag via jj git push --tag.
echo "Pushing tag ${TAG} to remote…"
jj git push --tag "${TAG}" || die "failed to push tag ${TAG}; ensure the 'origin' remote is configured"

echo ""
echo "✓ Tag ${TAG} pushed."
echo "  The release workflow will build and publish the GitHub Release automatically."
echo "  Track progress: https://github.com/TimoFreiberg/grindbot/actions"
echo ""
echo "  Once the release completes, verify with:"
echo "    cargo binstall grindbot"
