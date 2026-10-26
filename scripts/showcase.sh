#!/usr/bin/env bash
# Demo profiles for README screenshots, kept apart from your real ones.
#
# Everything lives in its own COLIMA_HOME (~/.colima-showcase by default), so `colima list`
# there shows only these profiles, and the app run with that COLIMA_HOME does the same.
#
#   scripts/showcase.sh up     create and fill the demo profiles (safe to rerun)
#   scripts/showcase.sh run    open the app on the demo profiles
#   scripts/showcase.sh down   delete the demo profiles and their folder
#
# Profiles:
#   web     docker, running: an app stack plus stopped containers, a dangling image and
#           unused volumes, so Overview, Containers, Images, Volumes and Cleanup all have data
#   build   containerd, stopped
#
# `colima start` switches the current Docker context to the new profile; `up` switches it
# back to whatever it was before.
set -euo pipefail

export COLIMA_HOME="${SHOWCASE_HOME:-$HOME/.colima-showcase}"
# Colima ignores COLIMA_HOME when the folder doesn't exist and falls back to ~/.colima.
mkdir -p "$COLIMA_HOME"
PROFILES=(web build)

say() { printf '\033[1m==> %s\033[0m\n' "$*"; }

context_name() {
    if [ "$1" = default ]; then echo colima; else echo "colima-$1"; fi
}

# Colima writes a Docker context named after the profile into ~/.docker, which is shared with
# your real setup. Refuse to touch one that isn't ours.
check_contexts() {
    for profile in "${PROFILES[@]}"; do
        local name endpoint
        name=$(context_name "$profile")
        endpoint=$(docker context inspect "$name" --format '{{.Endpoints.docker.Host}}' 2>/dev/null) || continue
        case "$endpoint" in
            "unix://$COLIMA_HOME/"*) ;;
            *)
                echo "Docker context '$name' already exists and points at $endpoint." >&2
                echo "Rename the showcase profiles in scripts/showcase.sh to avoid it." >&2
                exit 1
                ;;
        esac
    done
}

running() {
    colima status -p "$1" >/dev/null 2>&1
}

exists() {
    [ -f "$COLIMA_HOME/$1/colima.yaml" ]
}

# Stops if colima put the profile anywhere but COLIMA_HOME.
check_landed() {
    if ! exists "$1"; then
        echo "Profile '$1' was not created in $COLIMA_HOME; check 'colima list' for a stray one." >&2
        exit 1
    fi
}

d() {
    docker --host "unix://$COLIMA_HOME/web/docker.sock" "$@"
}

has_container() {
    [ -n "$(d ps -aq --filter "name=^$1\$")" ]
}

restore_context() {
    if [ -n "${PREVIOUS_CONTEXT:-}" ]; then
        docker context use "$PREVIOUS_CONTEXT" >/dev/null 2>&1 || true
    fi
}

up() {
    check_contexts
    PREVIOUS_CONTEXT=$(docker context show 2>/dev/null || true)
    trap restore_context EXIT

    if ! running web; then
        say "Starting profile 'web' (docker)"
        colima start -p web --runtime docker --cpus 2 --memory 2 --disk 20
    fi
    check_landed web

    if ! exists build; then
        say "Creating profile 'build' (containerd), then stopping it"
        colima start -p build --runtime containerd --cpus 1 --memory 1 --disk 10
        check_landed build
        colima stop -p build
    fi

    say "Pulling images"
    for image in nginx:1.27-alpine redis:7-alpine postgres:16-alpine alpine:3.20 busybox:1.36 \
        python:3.12-alpine; do
        d image inspect "$image" >/dev/null 2>&1 || d pull -q "$image"
    done

    say "Creating containers"
    d volume create shop-pgdata >/dev/null
    d volume create old-cache >/dev/null
    d volume create scratch >/dev/null
    has_container shop-web ||
        d run -d --name shop-web -p 18080:80 nginx:1.27-alpine >/dev/null
    has_container shop-cache ||
        d run -d --name shop-cache redis:7-alpine >/dev/null
    has_container shop-db ||
        d run -d --name shop-db -e POSTGRES_PASSWORD=showcase \
            -v shop-pgdata:/var/lib/postgresql/data postgres:16-alpine >/dev/null
    has_container shop-migrate ||
        d run --name shop-migrate alpine:3.20 echo "migrations applied" >/dev/null
    has_container nightly-report ||
        d run --name nightly-report busybox:1.36 sh -c 'echo report; exit 1' >/dev/null || true

    # Two commits to the same tag leave the first one dangling, for the Cleanup tab.
    if [ -z "$(d images -q --filter dangling=true)" ]; then
        say "Making a dangling image"
        for build in 1 2; do
            d run --name showcase-build alpine:3.20 sh -c "echo build $build > /build" >/dev/null
            d commit showcase-build showcase/worker:dev >/dev/null
            d rm showcase-build >/dev/null
        done
    fi

    say "Ready. Run 'just showcase' or 'scripts/showcase.sh run' to open the app."
}

run() {
    if ! exists web; then
        echo "No showcase profiles yet. Run 'scripts/showcase.sh up' first." >&2
        exit 1
    fi
    cd "$(dirname "$0")/.."
    exec cargo run
}

down() {
    for profile in "${PROFILES[@]}"; do
        if exists "$profile"; then
            say "Deleting profile '$profile'"
            colima delete -p "$profile" --force
        fi
    done
    case "$COLIMA_HOME" in
        "$HOME/.colima" | "$HOME/.colima/" | "$HOME" | "/") ;;
        *) rm -rf "$COLIMA_HOME" ;;
    esac
    # Colima adds an Include for each COLIMA_HOME to ~/.ssh/config and leaves it behind.
    local include="Include $COLIMA_HOME/ssh_config" config="$HOME/.ssh/config"
    if [ -f "$config" ] && grep -qxF "$include" "$config"; then
        grep -vxF "$include" "$config" >"$config.showcase" || true
        cat "$config.showcase" >"$config"
        rm "$config.showcase"
    fi
    say "Showcase removed."
}

case "${1:-}" in
    up) up ;;
    run) run ;;
    down) down ;;
    *)
        echo "usage: $0 up|run|down" >&2
        exit 2
        ;;
esac
