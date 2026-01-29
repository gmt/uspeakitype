# KDE Plasma headless Docker image for testing fcitx5_bridge
FROM debian:trixie-slim

# KDE minimal + fcitx5 + build tools
RUN apt-get update && apt-get install -y --no-install-recommends \
    # KDE Wayland (minimal)
    kwin-wayland kwin-wayland-backend-virtual \
    # fcitx5
    fcitx5 fcitx5-modules libfcitx5core-dev libfcitx5utils-dev \
    # Build tools for our addon
    build-essential cmake pkg-config \
    # Rust
    curl ca-certificates \
    # D-Bus (required)
    dbus dbus-x11 \
    # Misc
    procps \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain stable --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"

# Create runtime directories
RUN mkdir -p /run/user/0 && chmod 700 /run/user/0 && \
    mkdir -p /run/dbus

# Environment
ENV XDG_RUNTIME_DIR=/run/user/0 \
    WAYLAND_DISPLAY=wayland-0 \
    DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/0/bus \
    HOME=/root

# Copy usit source
WORKDIR /app
COPY . .

# Build fcitx5-usit-bridge addon
RUN cd fcitx5-usit-bridge && \
    mkdir -p build && cd build && \
    cmake .. && make && \
    mkdir -p /root/.local/lib/fcitx5 /root/.local/share/fcitx5/addon && \
    cp src/libusitbridge.so /root/.local/lib/fcitx5/ && \
    cp src/usitbridge.conf /root/.local/share/fcitx5/addon/

# Build usit
RUN cargo build --release

# Test script
COPY docker/kde/test-kde.sh /test-kde.sh
RUN chmod +x /test-kde.sh

CMD ["/test-kde.sh"]
