# Generic baseline x86-64 build container for lan-mouse.
# Produces a portable binary safe for any x86_64 host (fixes gcc -march=znver3 AVX SIGILL).
FROM archlinux:base-devel

RUN pacman -Syu --noconfirm rust pkgconf gtk4 libadwaita libx11 libxtst && paccache -r >/dev/null 2>&1 || true

WORKDIR /src
COPY . .

# Pin baseline ISA for both Rust and any C code compiled by cc (ring/webrtc-dtls)
ENV CFLAGS="-O2 -march=x86-64 -mtune=generic" \
    CXXFLAGS="-O2 -march=x86-64 -mtune=generic" \
    RUSTFLAGS="-C target-cpu=x86-64"

RUN cargo build --release --locked && strip target/release/lan-mouse
