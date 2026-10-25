#!/bin/sh
# All paths arrive as positional arguments, never as shell source.
pid=$1
app=$2
new=$3
tmp=$4
old="$app.old"
ready="$tmp/ready"
staged=$new
# Do not wait forever if the original process could not quit.
i=0
while /bin/kill -0 "$pid" 2>/dev/null; do
    i=$((i + 1))
    if [ "$i" -gt 120 ]; then /bin/rm -rf "$staged" "$tmp"; exit 1; fi
    /bin/sleep 1
done
rollback() {
    if [ -f "$ready" ]; then /bin/kill "$(/bin/cat "$ready")" 2>/dev/null || :; fi
    # A process that hung before acknowledgement must not survive the rollback.
    # Restrict lsof to processes mapping this exact executable as program text.
    for candidate in $(/usr/sbin/lsof -a -d txt -t "$app/Contents/MacOS/colima-gpui" 2>/dev/null); do
        /bin/kill "$candidate" 2>/dev/null || :
    done
    /bin/rm -rf "$app" || exit 1
    /bin/mv "$old" "$app" || exit 1
    /bin/rm -rf "$staged"
    /usr/bin/open -n "$app" --args --colima-update-failed "$1"
    # Keep the private log for diagnosis of a failed handoff.
    exit 1
}
[ ! -e "$old" ] && [ ! -L "$old" ] || { /bin/rm -rf "$staged"; /usr/bin/open -n "$app" --args --colima-update-failed; exit 1; }
/bin/mv "$app" "$old" || { /bin/rm -rf "$staged"; /usr/bin/open -n "$app" --args --colima-update-failed; exit 1; }
/bin/mv "$staged" "$app" || rollback "Could not move the new app into place."
/usr/bin/open -n "$app" --args --colima-update-ready "$ready" || rollback "macOS could not open the new app."
i=0
while [ ! -s "$ready" ]; do
    i=$((i + 1))
    [ "$i" -le 60 ] || rollback "The new app did not finish starting within 60 seconds."
    /bin/sleep 1
done
/bin/kill -0 "$(/bin/cat "$ready")" 2>/dev/null || rollback "The new app exited during startup."
/bin/rm -rf "$old" "$tmp"
