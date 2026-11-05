#!/usr/bin/env bash
set -euo pipefail

: "${TAG:?TAG must be the release tag}"
version="$(cargo metadata --no-deps --locked --format-version 1 | jq -er '
  [.packages[] | select(.name == "document-graphics-mermaid")]
  | if length == 1 then .[0].version else error("expected the fork package") end
')"
if [[ "$TAG" != "document-graphics-mermaid-v${version}" ]]; then
  echo "::error::Tag does not match document-graphics-mermaid ${version}" >&2
  exit 1
fi
if ! git merge-base --is-ancestor HEAD refs/remotes/origin/main; then
  echo "::error::Tagged commit must be on origin/main; push main before tagging" >&2
  exit 1
fi
