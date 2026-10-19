#!/usr/bin/env bash
# Cut the next server prerelease: tag the tip of origin/staging as
# vX.Y.Z-staging.N and push the tag, which starts release-linux.yml.
#
#   make release-staging              # tag and push
#   make release-staging DRY_RUN=1    # say which tag and commit, push nothing
#   make release-staging NEXT=0.2.0   # start the series for a minor or major bump
#   make release-staging FORCE=1      # tag without a green ci.yml run on the commit
#
# What it holds to, because each has gone wrong by hand:
#
# - The prefix is the NEXT stable, never the current one. A prerelease sorts
#   below its own release, so once vA.B.C is stable every vA.B.C-staging.N is a
#   downgrade that `virtues upgrade --pre` refuses, and prerelease servers sit
#   with nothing newer. After vA.B.C ships the series is vA.B.(C+1)-staging.N,
#   unless a higher series has already been started.
# - N counts up across prefixes, so of two tags the higher N is the later build.
# - The tag goes on origin/staging, the commit the PR merge left there. Never on
#   HEAD: in the shared checkout HEAD is `wave`, with other agents' work in it.
# - ci.yml has passed on that commit. Staging has no required checks, so a merge
#   can land while they are still running.
# - Tags are read from the remote, not from this clone's tag list, which can be
#   missing ones another machine pushed.

set -euo pipefail

remote=origin
die() { echo "release-staging: $*" >&2; exit 1; }

git fetch --quiet "$remote" staging
commit=$(git rev-parse "refs/remotes/$remote/staging^{commit}")

# "<commit> <tag>" per tag. ls-remote lists an annotated tag twice, the second
# time peeled to its commit with a ^{} suffix; the peeled line comes last, so
# it is the one kept.
remote_tags=$(git ls-remote --tags "$remote" | awk '
    { tag = $2; sub("^refs/tags/", "", tag); sub("\\^\\{\\}$", "", tag)
      sha[tag] = $1 }
    END { for (t in sha) print sha[t], t }')

semver_sort() { sort -t. -k1,1n -k2,2n -k3,3n; }

stable=$(printf '%s\n' "$remote_tags" | awk '{print $2}' \
    | sed -nE 's/^v([0-9]+\.[0-9]+\.[0-9]+)$/\1/p' | semver_sort | tail -1)
staging_bases=$(printf '%s\n' "$remote_tags" | awk '{print $2}' \
    | sed -nE 's/^v([0-9]+\.[0-9]+\.[0-9]+)-staging\.[0-9]+$/\1/p' | semver_sort)
top_base=$(printf '%s\n' "$staging_bases" | tail -1)
last_n=$(printf '%s\n' "$remote_tags" | awk '{print $2}' \
    | sed -nE 's/^v[0-9]+\.[0-9]+\.[0-9]+-staging\.([0-9]+)$/\1/p' | sort -n | tail -1)

# 1 when $1 is a higher X.Y.Z than $2.
newer() { [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | semver_sort | tail -1)" = "$1" ]; }

if [ -n "${NEXT:-}" ]; then
    printf '%s' "$NEXT" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$' \
        || die "NEXT=$NEXT is not X.Y.Z"
    base=$NEXT
elif [ -n "$stable" ]; then
    base=$(printf '%s' "$stable" | awk -F. '{ printf "%d.%d.%d", $1, $2, $3 + 1 }')
    if [ -n "$top_base" ] && newer "$top_base" "$base"; then base=$top_base; fi
elif [ -n "$top_base" ]; then
    base=$top_base
else
    die "no stable or staging tag on $remote to count from; name the series with NEXT=X.Y.Z"
fi

if [ -n "$stable" ] && ! newer "$base" "$stable"; then
    die "v$base-staging.N would sort below the stable v$stable, and prerelease servers would refuse it; use NEXT above $stable"
fi

tag="v$base-staging.$(( ${last_n:-0} + 1 ))"
echo "origin/staging is $(git log -1 --format='%h %s' "$commit")"
echo "newest stable:    ${stable:+v$stable}"
echo "next prerelease:  $tag"

already=$(printf '%s\n' "$remote_tags" \
    | awk -v c="$commit" '$1 == c && $2 ~ /^v[0-9]+\.[0-9]+\.[0-9]+-staging\.[0-9]+$/ { print $2 }')
if [ -n "$already" ] && [ "${FORCE:-}" != 1 ]; then
    die "origin/staging is already released as $already; merge new work into staging first, or FORCE=1"
fi

# ci.yml's push run on the merge commit. A docs-only merge skips ci.yml (its
# paths-ignore), so there is no run to find; FORCE=1 tags it anyway.
if [ "${FORCE:-}" != 1 ]; then
    command -v gh >/dev/null || die "needs the gh CLI to check CI on $commit, or FORCE=1"
    runs=$(gh run list --workflow ci.yml --commit "$commit" --json status,conclusion \
        --jq '.[] | "\(.status) \(.conclusion)"')
    if printf '%s\n' "$runs" | grep -q '^completed success$'; then
        echo "ci.yml:           passed on $(git rev-parse --short "$commit")"
    elif printf '%s\n' "$runs" | grep -qvE '^completed|^$'; then
        die "ci.yml is still running on $commit; wait for it to pass"
    elif [ -n "$runs" ]; then
        die "ci.yml did not pass on $commit; fix staging first, or FORCE=1"
    else
        die "no ci.yml run on $commit (a docs-only merge skips CI); FORCE=1 to tag anyway"
    fi
fi

if [ "${DRY_RUN:-}" = 1 ]; then
    echo "DRY_RUN=1: would push $tag at $commit"
    exit 0
fi

git push "$remote" "$commit:refs/tags/$tag"
echo "pushed $tag. release-linux.yml builds it:"
echo "  gh run list --workflow release-linux.yml --limit 1"
echo "then bring the merge back into wave:  git fetch $remote && git merge $remote/staging"
