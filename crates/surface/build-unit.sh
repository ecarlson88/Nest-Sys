#!/bin/sh
# nest-surface for the thermostat: a static ARMv7 binary into
# build/nest-surface-armv7, built as nestlocal is (cargo-zigbuild, the
# thermostat profile; BUILDING.md lists the tools).
#
#     sh crates/surface/build-unit.sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"
target=armv7-unknown-linux-musleabihf
target_dir=${SURFACE_TARGET_DIR:-.work/surface-target}
command -v rustup > /dev/null && rustup target add "$target" > /dev/null 2>&1 || true
cargo zigbuild --locked -p surface --bin nest-surface --profile thermostat --target "$target" --target-dir "$target_dir"
mkdir -p build
cp "$target_dir/$target/thermostat/nest-surface" build/nest-surface-armv7
echo "build/nest-surface-armv7"
