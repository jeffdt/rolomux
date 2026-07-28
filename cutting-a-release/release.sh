#!/usr/bin/env bash
# Shared release helper for jeffdt's TUI apps. See this skill's SKILL.md for
# the full workflow this automates.
#
#   release.sh bump <patch|minor|major>
#       Run on the feature branch, before merging its PR. Bumps Cargo.toml,
#       refreshes Cargo.lock, commits. That commit rides in the PR as usual.
#
#   release.sh cut
#       Run after that PR has merged into main. Reads the version already
#       committed there (no bump-type decision left to make), tags, waits
#       for release.yml, hashes the asset, updates jeffdt/homebrew-tap, and
#       upgrades the local install.
#
# Per-app naming (asset name, Homebrew formula name) defaults to the
# Cargo.toml package name. Override either with a [package.metadata.tui-utils]
# table in the app's own Cargo.toml, needed when the package name doesn't
# match what ships (e.g. teleport's package is "tp-core" but its formula and
# tap-dir env var are "tp"):
#
#   [package.metadata.tui-utils]
#   asset_name = "tp-core"     # release asset / binary invoked at the end
#   formula_name = "tp"        # Homebrew formula name + tap-dir env prefix
#
# The tap dir itself defaults to ~/code/homebrew-tap; override by setting
# <FORMULA_NAME_UPPER>_TAP_DIR (e.g. ROLOMUX_TAP_DIR, TP_TAP_DIR).

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
CARGO_TOML="$REPO_ROOT/Cargo.toml"

METADATA="$(cd "$REPO_ROOT" && cargo metadata --no-deps --format-version 1)"
PKG_NAME="$(jq -r '.packages[0].name' <<< "$METADATA")"
ASSET_BASE="$(jq -r --arg default "$PKG_NAME" '.packages[0].metadata."tui-utils".asset_name // $default' <<< "$METADATA")"
FORMULA_NAME="$(jq -r --arg default "$PKG_NAME" '.packages[0].metadata."tui-utils".formula_name // $default' <<< "$METADATA")"
ASSET="${ASSET_BASE}-aarch64-apple-darwin"
BINARY_NAME="$ASSET_BASE"

FORMULA_NAME_UPPER="$(echo "$FORMULA_NAME" | tr '[:lower:]-' '[:upper:]_')"
TAP_DIR_VAR="${FORMULA_NAME_UPPER}_TAP_DIR"
TAP_DIR="${!TAP_DIR_VAR:-$HOME/code/homebrew-tap}"

current_version() {
    grep -m1 '^version = ' "$CARGO_TOML" | sed -E 's/version = "(.*)"/\1/'
}

next_version() {
    local kind="$1" ver major minor patch
    ver="$(current_version)"
    IFS='.' read -r major minor patch <<< "$ver"
    case "$kind" in
        major) major=$((major + 1)); minor=0; patch=0 ;;
        minor) minor=$((minor + 1)); patch=0 ;;
        patch) patch=$((patch + 1)) ;;
        *) echo "error: bump kind must be patch, minor, or major" >&2; exit 1 ;;
    esac
    echo "$major.$minor.$patch"
}

cmd_bump() {
    local kind="${1:?usage: release.sh bump <patch|minor|major>}"
    local old new
    old="$(current_version)"
    new="$(next_version "$kind")"
    echo "==> Bumping $old -> $new ($kind)"
    sed -i '' -E "s/^version = \"$old\"/version = \"$new\"/" "$CARGO_TOML"
    (cd "$REPO_ROOT" && cargo build --release)
    git -C "$REPO_ROOT" add Cargo.toml Cargo.lock
    git -C "$REPO_ROOT" commit -m "Bump version to $new"
    echo "==> Committed. Include this commit in the feature PR; run 'release.sh cut' after it merges to main."
}

cmd_cut() {
    cd "$REPO_ROOT"

    local branch
    branch="$(git branch --show-current)"
    if [[ "$branch" != "main" ]]; then
        echo "error: must be on main (currently on $branch)" >&2
        exit 1
    fi
    if [[ -n "$(git status --porcelain)" ]]; then
        echo "error: working tree not clean" >&2
        exit 1
    fi

    echo "==> Pulling main"
    git pull --ff-only

    local version tag
    version="$(current_version)"
    tag="v$version"

    if git rev-parse "$tag" >/dev/null 2>&1; then
        echo "error: tag $tag already exists" >&2
        exit 1
    fi

    echo "==> Tagging $tag on $(git rev-parse --short HEAD)"
    git tag -a "$tag" -m "Release $version"
    git push origin "$tag"

    echo "==> Waiting for release.yml to start"
    local run_id=""
    for _ in $(seq 1 10); do
        run_id="$(gh run list --workflow=release.yml --limit 5 --json databaseId,headBranch \
            -q ".[] | select(.headBranch == \"$tag\") | .databaseId" | head -1)"
        [[ -n "$run_id" ]] && break
        sleep 3
    done
    if [[ -z "$run_id" ]]; then
        echo "error: no release.yml run showed up for $tag after 30s" >&2
        exit 1
    fi
    gh run watch "$run_id" --exit-status

    echo "==> Downloading asset and hashing"
    local tmpdir sha
    tmpdir="$(mktemp -d)"
    gh release download "$tag" -p "$ASSET" -D "$tmpdir"
    sha="$(shasum -a 256 "$tmpdir/$ASSET" | awk '{print $1}')"
    echo "    sha256: $sha"

    if [[ ! -d "$TAP_DIR" ]]; then
        echo "error: tap not found at $TAP_DIR (set $TAP_DIR_VAR)" >&2
        exit 1
    fi

    echo "==> Updating $TAP_DIR/Formula/$FORMULA_NAME.rb"
    (cd "$TAP_DIR" && git pull --ff-only)
    local formula="$TAP_DIR/Formula/$FORMULA_NAME.rb"
    sed -i '' -E "s#download/v[0-9]+\.[0-9]+\.[0-9]+/$ASSET#download/$tag/$ASSET#" "$formula"
    sed -i '' -E "s/sha256 \"[a-f0-9]+\"/sha256 \"$sha\"/" "$formula"

    echo "==> Validating formula"
    (cd "$TAP_DIR" && brew style jeffdt/tap)
    (cd "$TAP_DIR" && brew readall --aliases --os=all --arch=all jeffdt/tap)
    (cd "$TAP_DIR" && brew audit --except=installed --tap=jeffdt/tap)

    echo "==> Pushing tap"
    (cd "$TAP_DIR" && git add "Formula/$FORMULA_NAME.rb" && git commit -m "Bump $FORMULA_NAME to $version" && git push)

    echo "==> Upgrading local install"
    brew update
    brew upgrade "jeffdt/tap/$FORMULA_NAME"
    "$BINARY_NAME" --version

    echo "==> Done. $FORMULA_NAME $version is live."
}

case "${1:-}" in
    bump) shift; cmd_bump "$@" ;;
    cut) cmd_cut ;;
    *)
        echo "usage: $0 {bump <patch|minor|major>|cut}" >&2
        exit 1
        ;;
esac
