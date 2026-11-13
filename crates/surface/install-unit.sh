#!/bin/sh
# Copies the control surface to a rooted unit over SSH (unfr0zen --ssh),
# into /media/data/nest-surface: nest-surface (from build-unit.sh), its
# launcher, init script and monit file, and config.json with the Core's
# address when one is given. Nothing on the unit's root file system is
# changed; README.md has the steps to try it and to start it at boot.
#
#     sh crates/surface/install-unit.sh UNIT_ADDRESS [CORE_ADDRESS]
#
# SSH_OPTS adds options for ssh (the gen 3's OpenSSH 6.0 may need some on
# a newer computer, e.g. SSH_OPTS="-o HostKeyAlgorithms=+ssh-rsa").
set -eu
[ $# -ge 1 ] || { echo "usage: $0 UNIT_ADDRESS [CORE_ADDRESS]" >&2; exit 2; }
unit=$1
core=${2:-}
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
bin=$root/build/nest-surface-armv7
[ -f "$bin" ] || { echo "$bin is missing: run crates/surface/build-unit.sh first" >&2; exit 1; }
dir=/media/data/nest-surface
# shellcheck disable=SC2086
on_unit() { ssh ${SSH_OPTS:-} "root@$unit" "$@"; }
put() {  # put LOCAL REMOTE MODE
    on_unit "cat > '$2.new' && chmod $3 '$2.new' && mv '$2.new' '$2'" < "$1"
}

on_unit "mkdir -p $dir"
put "$bin" "$dir/nest-surface" 755
put "$root/crates/surface/rootfs/nest-surface.sh" "$dir/nest-surface.sh" 755
put "$root/crates/surface/rootfs/nest-surface.init" "$dir/nest-surface.init" 755
put "$root/crates/surface/rootfs/nest-surface.monitrc" "$dir/nest-surface.monitrc" 600
if [ -n "$core" ]; then
    printf '{"core": "%s"}\n' "$core" | on_unit "cat > $dir/config.json"
fi
echo "Installed in $unit:$dir. To try it: ssh root@$unit sh $dir/nest-surface.sh (Ctrl-C to stop)."
