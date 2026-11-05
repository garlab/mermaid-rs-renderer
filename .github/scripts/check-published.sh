#!/usr/bin/env bash
set -euo pipefail

: "${TAG:?TAG must be the validated release tag}"
version="${TAG#document-graphics-mermaid-v}"
if [[ "$version" == "$TAG" || -z "$version" ]]; then
  echo "::error::Expected a package-prefixed release tag" >&2
  exit 1
fi
status="$(curl --silent --show-error --location \
  --connect-timeout 10 --max-time 30 --retry 3 \
  --output /dev/null --write-out '%{http_code}' \
  --user-agent 'document-graphics-mermaid release (github.com/garlab/document-graphics-mermaid)' \
  "https://crates.io/api/v1/crates/document-graphics-mermaid/${version}")"
case "$status" in
  200) echo 'published=true' ;;
  404) echo 'published=false' ;;
  *) echo "::error::Could not check crates.io version (HTTP ${status})" >&2; exit 1 ;;
esac
