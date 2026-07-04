#!/usr/bin/env bash
# Release helper mirroring AGENTS.md's "Cutting a release" workflow.
#
#   scripts/release.sh bump <patch|minor|major>
#       Run on the feature branch, before merging its PR. Bumps Cargo.toml,
#       refreshes Cargo.lock, commits. The bump then rides in the PR as
#       AGENTS.md requires.
#
#   scripts/release.sh cut
#       Run after that PR has merged into main. Reads the version already
#       committed there (no bump-type decision left to make), tags, waits
#       for release.yml, hashes the asset, updates jeffdt/homebrew-tap,
#       and upgrades the local install.
#
# Set ROLOMUX_TAP_DIR if the tap isn't checked out at ~/code/homebrew-tap.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
CARGO_TOML="$REPO_ROOT/Cargo.toml"
TAP_DIR="${ROLOMUX_TAP_DIR:-$HOME/code/homebrew-tap}"
ASSET="rolomux-aarch64-apple-darwin"

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
        echo "error: tap not found at $TAP_DIR (set ROLOMUX_TAP_DIR)" >&2
        exit 1
    fi

    echo "==> Updating $TAP_DIR/Formula/rolomux.rb"
    (cd "$TAP_DIR" && git pull --ff-only)
    local formula="$TAP_DIR/Formula/rolomux.rb"
    sed -i '' -E "s#download/v[0-9]+\.[0-9]+\.[0-9]+/$ASSET#download/$tag/$ASSET#" "$formula"
    sed -i '' -E "s/sha256 \"[a-f0-9]+\"/sha256 \"$sha\"/" "$formula"

    echo "==> Validating formula"
    (cd "$TAP_DIR" && brew style jeffdt/tap)
    (cd "$TAP_DIR" && brew readall --aliases --os=all --arch=all jeffdt/tap)
    (cd "$TAP_DIR" && brew audit --except=installed --tap=jeffdt/tap)

    echo "==> Pushing tap"
    (cd "$TAP_DIR" && git add Formula/rolomux.rb && git commit -m "Bump rolomux to $version" && git push)

    echo "==> Upgrading local install"
    brew update
    brew upgrade jeffdt/tap/rolomux
    rolomux --version

    echo "==> Done. rolomux $version is live."
}

case "${1:-}" in
    bump) shift; cmd_bump "$@" ;;
    cut) cmd_cut ;;
    *)
        echo "usage: $0 {bump <patch|minor|major>|cut}" >&2
        exit 1
        ;;
esac
