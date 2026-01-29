# KDE Plasma headless Docker image for testing fcitx5_bridge and audio capture
FROM debian:trixie-slim

# KDE + fcitx5 + PipeWire + build deps
RUN apt-get update && apt-get install -y --no-install-recommends \
    # KDE Wayland (virtual backend is built-in)
    kwin-wayland \
    # fcitx5
    fcitx5 fcitx5-modules fcitx5-modules-dev libfcitx5core-dev libfcitx5utils-dev \
    # Build tools
    build-essential cmake pkg-config libssl-dev libclang-dev \
    # Wayland dev
    libwayland-dev libxkbcommon-dev \
    # Graphics (software rendering)
    mesa-vulkan-drivers libvulkan1 libegl1 libgl1-mesa-dri \
    # Audio libs (dev)
    libasound2-dev libpipewire-0.3-dev \
    # PipeWire runtime (for audio capture tests)
    pipewire pipewire-audio-client-libraries wireplumber \
    # Rust installer
    curl ca-certificates \
    # D-Bus
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
