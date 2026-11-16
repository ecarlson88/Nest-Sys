#!/bin/sh
# Runs the Q-SYS control surface in place of the thermostat's screen, as
# Doom's launcher does (crates/nestlocal/rootfs/doom.sh), until stopped.
#
# An nlclient patched with the screen hand-over leaves its pid first in
# keeper.pid: it keeps running (and controlling the heating) and shows
# nothing while the process in screen.owner, this shell, is alive. It also
# lights the display at its own brightness every 2 s, so the surface leaves
# the backlight to it then (--no-backlight). Any other nlclient, or any with
# --exclusive, is stopped while the surface runs and started again after:
# no thermostat meanwhile, but the surface's brightness, dimming and screen
# off work.
#
# The program is /nestlabs/sbin/nest-surface, on the root file system as
# unfr0zen's own are (/media/data is mounted noexec). Its other files are in
# /media/data/nest-surface: this launcher, config.json (the Core's address)
# and settings.json. Its log is
# /tmp/nest-surface.log. Arguments are passed on to nest-surface; with
# --diag (no Core needed), its report goes to this terminal instead.
bin=/nestlabs/sbin/nest-surface
# --exclusive is the launcher's; the rest go to nest-surface.
exclusive=
for arg in "$@"; do
    shift
    if [ "$arg" = --exclusive ]; then exclusive=1; else set -- "$@" "$arg"; fi
done
run=/var/run/unfr0zen
log=/tmp/nest-surface.log

surface() {
    if [ "${1:-}" = --diag ]; then
        "$bin" "$@" &
    else
        "$bin" "$@" >> "$log" 2>&1 &
    fi
    child=$!
    # Stopped (by monit, init or Ctrl-C), stop the surface, which puts the
    # display back before it exits.
    trap 'kill $child 2> /dev/null' TERM INT HUP
    wait $child
    wait $child 2> /dev/null
}

read -r keeper _ 2> /dev/null < "$run/keeper.pid"
if [ -z "$exclusive" ] && [ -n "$keeper" ] && [ "$keeper" = "$(pidof nlclient)" ]; then
    echo "nest-surface.sh: nlclient hands over the screen and keeps running; the backlight stays its own" | tee -a "$log"
    echo $$ > "$run/screen.owner"
    surface "$@" --no-backlight
    rm -f "$run/screen.owner"
    exit 0
fi
if [ -n "$exclusive" ]; then why="--exclusive"; else why="no hand-over from nlclient"; fi
echo "nest-surface.sh: $why: stopping nlclient (the thermostat is not running) until this stops" | tee -a "$log"
monit unmonitor nlclient > /dev/null 2>&1
for i in 1 2 3 4 5 6 7 8 9 10; do
    pid=$(pidof nlclient) || break
    if [ "$i" -lt 6 ]; then kill $pid; else kill -9 $pid; fi
    sleep 1
done
surface "$@"
monit monitor nlclient > /dev/null 2>&1
monit start nlclient > /dev/null 2>&1
