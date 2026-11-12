fedora_image() {
    image="localhost/sourcedigger-fedora-$2:$1"
    if ! podman image exists "$image"; then
        podman pull --quiet --arch "$2" "registry.fedoraproject.org/fedora:$1" > /dev/null
        podman tag "registry.fedoraproject.org/fedora:$1" "$image"
    fi
    echo "$image"
}

architectures="${SOURCEDIGGER_ARCHITECTURES:-x86_64 aarch64}"
