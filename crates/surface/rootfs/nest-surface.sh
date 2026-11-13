#!/bin/sh
# Runs the Q-SYS control surface in place of the thermostat's screen, as
# Doom's launcher does (crates/nestlocal/rootfs/doom.sh), until stopped.
#
# An nlclient patched with the screen hand-over leaves its pid first in
# keeper.pid: it keeps running (and controlling the heating) and shows
# nothing while the process in screen.owner, this shell, is alive. Any other
# nlclient is stopped while the surface runs and started again after.
#
# The surface's files are in /media/data/nest-surface: nest-surface, this
# launcher, config.json (the Core's address) and settings.json. Its log is
# /tmp/nest-surface.log. Arguments are passed on to nest-surface.
dir=/media/data/nest-surface
run=/var/run/unfr0zen
log=/tmp/nest-surface.log

surface() {
    "$dir/nest-surface" "$@" >> "$log" 2>&1 &
    child=$!
    # Stopped (by monit, init or Ctrl-C), stop the surface, which puts the
    # display back before it exits.
    trap 'kill $child 2> /dev/null' TERM INT HUP
    wait $child
    wait $child 2> /dev/null
}

read -r keeper _ 2> /dev/null < "$run/keeper.pid"
if [ -n "$keeper" ] && [ "$keeper" = "$(pidof nlclient)" ]; then
    echo $$ > "$run/screen.owner"
    surface "$@"
    rm -f "$run/screen.owner"
    exit 0
fi
monit unmonitor nlclient > /dev/null 2>&1
for i in 1 2 3 4 5 6 7 8 9 10; do
    pid=$(pidof nlclient) || break
    if [ "$i" -lt 6 ]; then kill $pid; else kill -9 $pid; fi
    sleep 1
done
surface "$@"
monit monitor nlclient > /dev/null 2>&1
monit start nlclient > /dev/null 2>&1
