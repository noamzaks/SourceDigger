#!/bin/sh
set -eu

root=$(git rev-parse --show-toplevel)
. "$root/packaging/fedora-image.sh"
architectures="${SOURCEDIGGER_ARCHITECTURES:-$(uname -m)}"

run() {
    podman exec "$container" sh -euc "$1"
}

wait_for_server() {
    run 'for attempt in $(seq 60); do curl -sf localhost:2468/api > /dev/null && exit 0; sleep 1; done; exit 1'
}

version_count() {
    run 'curl -sf localhost:2468/api/sources/musl | python3 -c "import json, sys; print(json.load(sys.stdin)[\"version_count\"])"'
}

systemd_image() {
    image="localhost/sourcedigger-test-$2:$1"
    if ! podman image exists "$image"; then
        podman run --name "sourcedigger-test-image" "$(fedora_image "$1" "$2")" \
            dnf install --assumeyes --quiet systemd python3 > /dev/null 2>&1
        podman commit --quiet "sourcedigger-test-image" "$image" > /dev/null
        podman rm "sourcedigger-test-image" > /dev/null
    fi
    echo "$image"
}

test_release() {
    release=$1
    architecture=$2
    container="sourcedigger-test-fc$release-$architecture"
    package=$(find "$root/target/rpm/fc$release/$architecture" -name 'sourcedigger-[0-9]*.rpm')
    echo "Testing $(basename "$package")"
    podman run --detach --rm --name "$container" --volume="$(dirname "$package"):/rpms:Z" \
        "$(systemd_image "$release" "$architecture")" /sbin/init > /dev/null
    trap 'podman stop --time 1 "$container" > /dev/null 2>&1' EXIT
    run 'for attempt in $(seq 300); do case $(systemctl is-system-running) in running|degraded) exit 0;; esac; sleep 1; done; exit 1'

    run 'dnf install --assumeyes --quiet /rpms/sourcedigger-[0-9]*.rpm > /dev/null 2>&1'
    run 'systemctl enable --now sourcedigger 2> /dev/null'
    wait_for_server
    run 'curl -sf localhost:2468/ | grep -q "<title>SourceDigger</title>"'
    run 'curl -sf -X PUT localhost:2468/api/site -H "Content-Type: application/json" -d "{\"threads\": 2}" > /dev/null'
    run 'grep -q "threads = 2" /var/lib/sourcedigger/settings.toml'
    echo "  installed, the service serves the web interface and saves site settings"

    run 'curl -sf -X POST localhost:2468/api/sources -H "Content-Type: application/json" -d "{\"name\": \"musl\", \"url\": \"https://git.musl-libc.org/git/musl\", \"tag_pattern\": \"^v(1\\\\.2\\\\.[0-9]+)$\", \"file_pattern\": \"\\\\.[ch]$\", \"viewer_url\": \"\", \"refresh_hours\": null}" > /dev/null 2>&1'
    run 'curl -sf -X POST localhost:2468/api/sources/musl/digs > /dev/null 2>&1'
    run 'for attempt in $(seq 300); do curl -sf localhost:2468/api/sources/musl | grep -q "\"status\":\"succeeded\"" && exit 0; sleep 1; done; curl -s localhost:2468/api/sources/musl; exit 1'
    dug=$(version_count)
    echo "  dug $dug versions of musl into /var/lib/sourcedigger"

    run 'dnf reinstall --assumeyes --quiet /rpms/sourcedigger-[0-9]*.rpm > /dev/null 2>&1'
    wait_for_server
    test "$(version_count)" = "$dug"
    run 'systemctl is-active --quiet sourcedigger'
    echo "  reinstalled, the service restarted and kept its data"

    run 'dnf remove --assumeyes --quiet sourcedigger > /dev/null 2>&1'
    run '! systemctl is-active --quiet sourcedigger'
    run 'test ! -e /usr/lib/systemd/system/sourcedigger.service && test ! -e /usr/bin/sourcedigger-server'
    echo "  removed, the service stopped and its files are gone"

    run 'dnf install --assumeyes --quiet /rpms/sourcedigger-[0-9]*.rpm > /dev/null 2>&1'
    run 'systemctl enable --now sourcedigger 2> /dev/null'
    wait_for_server
    test "$(version_count)" = "$dug"
    echo "  installed again, the data dug before is still there"

    podman stop --time 1 "$container" > /dev/null
    trap - EXIT
}

if [ $# -eq 0 ]; then
    set -- 45
fi
for release in "$@"; do
    for architecture in $architectures; do
        test_release "$release" "$architecture"
    done
done
