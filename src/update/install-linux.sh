#!/bin/sh
# Paths are arguments. The staging file lives beside the destination, so rename is atomic.
pid=$1
app=$2
new=$3
tmp=$4
old="$app.old"
ready="$tmp/ready"
i=0
while kill -0 "$pid" 2>/dev/null; do
    i=$((i + 1))
    [ "$i" -le 120 ] || { rm -rf -- "$tmp"; exit 1; }
    sleep 1
done
launch() {
    # APPIMAGE is set afresh by the runtime, never inherit the previous mount path.
    (unset APPIMAGE APPDIR OWD; exec /usr/bin/setsid "$app" "$@") &
    child=$!
}
abort_install() {
    launch --colima-update-failed "$1"
    rm -rf -- "$tmp"
    exit 1
}
rollback() {
    /bin/kill -- "-$child" 2>/dev/null || :
    kill "$child" 2>/dev/null || :
    wait "$child" 2>/dev/null || :
    mv -f -- "$old" "$app" || exit 1
    launch --colima-update-failed "$1"
    rm -rf -- "$tmp"
    exit 1
}
[ ! -e "$old" ] && [ ! -L "$old" ] || abort_install 'An earlier backup exists.'
# A hard link retains the old inode while the rename replaces the public path atomically.
ln -- "$app" "$old" || abort_install 'Could not back up the installed AppImage.'
mv -f -- "$new" "$app" || { rm -f -- "$old"; abort_install 'Could not replace the installed AppImage.'; }
launch --colima-update-ready "$ready"
i=0
while [ ! -s "$ready" ]; do
    kill -0 "$child" 2>/dev/null || rollback 'The new AppImage exited during startup.'
    i=$((i + 1))
    [ "$i" -le 60 ] || rollback 'The new AppImage did not finish starting within 60 seconds.'
    sleep 1
done
# The runtime can supervise a child (notably with --appimage-extract-and-run).
# The acknowledgement contains the initialized UI process's PID.
ack=$(cat "$ready")
case "$ack" in ''|*[!0-9]*) rollback 'Invalid startup acknowledgement.' ;; esac
kill -0 "$ack" 2>/dev/null || rollback 'The new AppImage exited during startup.'
rm -f -- "$old"
rm -rf -- "$tmp"
