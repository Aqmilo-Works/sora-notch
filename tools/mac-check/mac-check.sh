#!/bin/sh
# Type-checks Sora for macOS on a Windows PC (Git Bash): `sh tools/mac-check/mac-check.sh` (or `... check --tests`).
# No Apple SDK is needed: build scripts get a stand-in C compiler (main.go) that writes empty objects, which is enough for
# `cargo check` (nothing is linked). Needs `rustup target add aarch64-apple-darwin` and Go once.
# The argfile keeps rustc's long command lines (objc2 crates) from failing to start on Windows.
set -e
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here/../.."
[ -x "$here/fakecc.exe" ] || (cd "$here" && go build -o fakecc.exe . && cp fakecc.exe fakear.exe)
export __CARGO_TEST_FORCE_ARGFILE=1 CC_aarch64_apple_darwin="$here/fakecc.exe" AR_aarch64_apple_darwin="$here/fakear.exe"
sub="${1:-check}"; [ $# -gt 0 ] && shift
exec cargo "$sub" --locked --target aarch64-apple-darwin --target-dir target/mac "$@"
