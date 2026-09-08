#!/usr/bin/env bash
# Download only when dfx actually builds this custom canister. Keep remote URLs
# out of candid/wasm in dfx.json: dfx 0.32 downloads those for unrelated targets.
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "Usage: $0 OUTPUT_DIRECTORY CANDID_URL WASM_URL" >&2
  exit 2
fi

output_dir=$1
mkdir -p "$output_dir"
temporary_file=''
trap 'if [ -n "$temporary_file" ]; then rm -f "$temporary_file"; fi' EXIT

download() {
  local url=$1 destination=$2
  # The directory in dfx.json is keyed by both URLs. Change it when updating
  # either URL, so an upgrade cannot silently reuse an older cached release.
  if [ -s "$destination" ]; then
    return
  fi
  temporary_file=$(mktemp "${destination}.XXXXXX")
  curl --fail --location --retry 3 --connect-timeout 30 \
    --output "$temporary_file" "$url"
  test -s "$temporary_file"
  mv "$temporary_file" "$destination"
  temporary_file=''
}

download "$2" "$output_dir/canister.did"
download "$3" "$output_dir/canister.wasm.gz"
