FROM debian:trixie-slim

# System dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    # Build tools
    build-essential pkg-config libssl-dev libclang-dev \
    # Rust (via rustup)
    curl ca-certificates \
    # Git (for staleness detection)
    git \
    # Wayland + Sway
    sway grim libwayland-dev libxkbcommon-dev \
    # Mesa software rendering (Vulkan + OpenGL)
    mesa-vulkan-drivers libvulkan1 vulkan-tools \
    libegl1 libgl1-mesa-dri mesa-utils \
    # Fonts (must match golden images)
    fonts-dejavu-core fonts-liberation fontconfig \
    # Audio libs (usit dependency)
    libasound2-dev libpipewire-0.3-dev \
    # Misc
    procps \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain stable --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"

# Font cache
RUN fc-cache -f -v

# Create runtime directory for Wayland
RUN mkdir -p /run/user/0 && chmod 700 /run/user/0

# Environment for headless Wayland + software rendering
ENV XDG_RUNTIME_DIR=/run/user/0 \
    WAYLAND_DISPLAY=wayland-0 \
    WLR_BACKENDS=headless \
    WLR_HEADLESS_OUTPUTS=1 \
    WLR_LIBINPUT_NO_DEVICES=1 \
    WLR_RENDERER=pixman \
    WLR_RENDERER_ALLOW_SOFTWARE=1 \
    LIBGL_ALWAYS_SOFTWARE=1 \
    VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json \
    USIT_CANONICAL_TEST_ENV=1

WORKDIR /app

# Copy source (or mount via docker-compose)
COPY . .

# Git state passed as build args (computed on host since .git is excluded)
ARG GIT_COMMIT=unknown
ARG GIT_DIFF_HASH=clean

# Store build state for staleness detection
RUN sha256sum Cargo.lock | cut -d' ' -f1 > /app/.cargo-lock-hash && \
    echo "$GIT_COMMIT" > /app/.git-commit && \
    echo "$GIT_DIFF_HASH" > /app/.git-diff-hash

# Build usit
RUN cargo build --release

# Test entrypoint
COPY scripts/docker-test.sh /docker-test.sh
RUN chmod +x /docker-test.sh

ENTRYPOINT ["/docker-test.sh"]
CMD ["cargo", "test", "--release", "--test", "visual_tests", "--", "--nocapture", "--test-threads=1"]
