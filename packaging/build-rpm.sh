#!/bin/sh
set -eu

root=$(git rev-parse --show-toplevel)
version=$(sed -n 's/^Version: *//p' "$root/packaging/sourcedigger.spec")
output="$root/target/rpm"
sources="$output/sources"

make_sources() {
    rm -rf "$sources"
    mkdir -p "$sources/vendor-tree"
    git -C "$root" archive --prefix="sourcedigger-$version/" --output="$sources/sourcedigger-$version.tar.gz" HEAD
    cargo vendor --locked --manifest-path "$root/Cargo.toml" "$sources/vendor-tree/vendor" > /dev/null
    tar --directory="$sources/vendor-tree" --create --gzip --file="$sources/sourcedigger-$version-vendor.tar.gz" vendor
    rm -rf "$sources/vendor-tree"
    npm ci --prefix "$root/web" --silent
    npm run build --prefix "$root/web" --silent
    tar --directory="$root" --create --gzip --file="$sources/sourcedigger-$version-web.tar.gz" web/dist
    cp "$root/packaging/sourcedigger.spec" "$sources/"
}

build_for() {
    release=$1
    echo "Building for Fedora $release"
    podman run --rm --volume="$output:/output:Z" "registry.fedoraproject.org/fedora:$release" sh -euc "
        dnf install --assumeyes --quiet rpm-build 'dnf-command(builddep)' > /dev/null
        dnf builddep --assumeyes --quiet /output/sources/sourcedigger.spec > /dev/null 2>&1
        rpmbuild -bb --quiet \
            --define '_sourcedir /output/sources' \
            --define '_rpmdir /output/fc$release' \
            /output/sources/sourcedigger.spec
    "
}

if [ $# -eq 0 ]; then
    set -- 44 45
fi
make_sources
for release in "$@"; do
    build_for "$release"
done
find "$output" -name '*.rpm'
