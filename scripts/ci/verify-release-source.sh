#!/usr/bin/env bash
# Release only commits already merged to main and validated by its push CI.
set -euo pipefail

: "${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
: "${GITHUB_SHA:?GITHUB_SHA is required}"
: "${GH_TOKEN:?GH_TOKEN with actions:read is required}"

if [[ "$(git rev-parse HEAD)" != "$GITHUB_SHA" ]]; then
  echo "::error::The checked-out commit does not match the release commit."
  exit 1
fi

# Callers use checkout with fetch-depth: 0 so ancestry checks are complete.
git fetch --no-tags origin main
if ! git merge-base --is-ancestor "$GITHUB_SHA" FETCH_HEAD; then
  echo "::error::The release commit must already be merged into main."
  exit 1
fi

run_id="$(gh api --method GET \
  "repos/$GITHUB_REPOSITORY/actions/workflows/test.yml/runs" \
  -f head_sha="$GITHUB_SHA" -f event=push -f status=success \
  -f per_page=100 | \
  jq -r --arg sha "$GITHUB_SHA" '.workflow_runs | map(select(.head_branch == "main" and .head_sha == $sha and .event == "push" and .conclusion == "success")) | first | .id // empty')"

if [[ -z "$run_id" ]]; then
  echo "::error::No successful main push CI exists for this commit. Wait for Test to pass, then rerun this release workflow."
  exit 1
fi

echo "Release source verified: $GITHUB_SHA (Test run $run_id)."
