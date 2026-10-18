#!/usr/bin/env bash
# Regenerates tests/parity/expected/ with the reference kepubify, so that
# `cargo test -p relio-kepub --test parity` checks relio-kepub against it.
#
# Uses $KEPUBIFY if set, otherwise builds the pinned version with Go.
set -euo pipefail

version=v4.0.4
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

kepubify=${KEPUBIFY:-}
if [ -z "$kepubify" ]; then
  GOBIN="$work/bin" GOFLAGS=-modcacherw go install "github.com/pgaskin/kepubify/v4/cmd/kepubify@$version"
  kepubify="$work/bin/kepubify"
fi
"$kepubify" --version | grep -q "${version}\|v4" || { echo "unexpected kepubify version" >&2; exit 1; }

rm -rf "$here/expected"
mkdir -p "$here/expected"
for case in "$here"/cases/*/; do
  name=$(basename "$case")
  epub="$work/$name.epub"
  (cd "$case" && zip -X0q "$epub" mimetype && zip -X9qrD "$epub" . -x mimetype -x kepubify.args)
  args=()
  if [ -f "$case/kepubify.args" ]; then
    while IFS= read -r line; do args+=("$line"); done < "$case/kepubify.args"
  fi
  "$kepubify" ${args[@]+"${args[@]}"} -o "$here/expected/$name.kepub.epub" "$epub" > /dev/null
  echo "$name"
done
echo "kepubify $version" > "$here/expected/VERSION"
