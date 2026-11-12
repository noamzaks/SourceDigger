#!/bin/sh
set -eu

root=$(git rev-parse --show-toplevel)
. "$root/packaging/fedora-image.sh"
version=$(sed -n 's/^Version: *//p' "$root/packaging/sourcedigger.spec")
output="$root/target/rpm"
sources="$output/sources"

sysroot() {
    path="$output/sysroot/fc$1-$2"
    if [ ! -d "$path" ]; then
        podman run --name sourcedigger-sysroot "$(fedora_image "$1" "$2")" \
            dnf install --assumeyes --quiet gcc glibc-devel zlib-devel > /dev/null 2>&1
        mkdir -p "$path"
        podman export sourcedigger-sysroot | tar --extract --directory="$path" --exclude='dev/*'
        podman rm sourcedigger-sysroot > /dev/null
    fi
    echo "$path"
}

cross_compile() {
    release=$1
    architecture=$2
    root_directory=$(sysroot "$release" "$architecture")
    target="$architecture-unknown-linux-gnu"
    variable=$(echo "$target" | tr - _)
    upper_variable=$(echo "$variable" | tr '[:lower:]' '[:upper:]')
    flags="--target=$architecture-redhat-linux --sysroot=$root_directory"
    env "CC_$variable=clang" "CFLAGS_$variable=$flags" \
        "CARGO_TARGET_${upper_variable}_LINKER=clang" \
        "CARGO_TARGET_${upper_variable}_RUSTFLAGS=-C link-arg=--target=$architecture-redhat-linux -C link-arg=--sysroot=$root_directory -C link-arg=-fuse-ld=lld" \
        PKG_CONFIG_ALLOW_CROSS=1 PKG_CONFIG_SYSROOT_DIR="$root_directory" \
        PKG_CONFIG_LIBDIR="$root_directory/usr/lib64/pkgconfig:$root_directory/usr/share/pkgconfig" \
        cargo build --quiet --release --locked --manifest-path "$root/Cargo.toml" --package sourcedigger-server \
        --target "$target" --target-dir "$output/cross-fc$release"
    mkdir -p "$output/fc$release"
    cp "$output/cross-fc$release/$target/release/sourcedigger-server" "$output/fc$release/sourcedigger-server.$architecture"
    echo "--define 'prebuilt_server /output/fc$release/sourcedigger-server.$architecture'"
}

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
    architecture=$2
    echo "Building for Fedora $release on $architecture"
    prebuilt=""
    if [ "$architecture" != "$(uname -m)" ]; then
        prebuilt=$(cross_compile "$release" "$architecture")
    fi
    podman run --rm --volume="$output:/output:Z" "$(fedora_image "$release" "$architecture")" sh -euc "
        dnf install --assumeyes --quiet rpm-build 'dnf-command(builddep)' > /dev/null
        dnf builddep --assumeyes --quiet /output/sources/sourcedigger.spec > /dev/null 2>&1
        rpmbuild -bb --quiet $prebuilt \
            --define '_sourcedir /output/sources' \
            --define '_rpmdir /output/fc$release' \
            /output/sources/sourcedigger.spec
    "
}

if ! git -C "$root" diff --quiet HEAD; then
    echo "Commit your changes first: the packages are built from the last commit" >&2
    exit 1
fi
if [ $# -eq 0 ]; then
    set -- 45
fi
make_sources
for release in "$@"; do
    for architecture in $architectures; do
        build_for "$release" "$architecture"
    done
done
find "$output" -name '*.rpm'
