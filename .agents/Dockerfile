# Disposable box for agent runs: sandbox bypass is safe here, not on your host.
# You don't run this directly: .agents/run builds the image on first use and
# re-executes itself inside it. Rebuild after editing: docker build -t mai-agents -f .agents/Dockerfile .agents
#
# Ubuntu 24.04 ships git 2.43; Debian bookworm (the rust image's base) is stuck at 2.39.
FROM ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config curl ca-certificates git gh python3 \
 && curl -fsSL https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal --default-toolchain 1.85.0 \
 && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
 && apt-get install -y --no-install-recommends nodejs \
 && npm install -g @openai/codex \
 && rm -rf /var/lib/apt/lists/*
RUN git config --system --add safe.directory '*'
# Agent harnesses may run commands in a login shell, which can reset PATH and drop the
# Rust toolchain. Expose it both ways: profile.d and /usr/local/bin.
RUN printf 'export RUSTUP_HOME=%s CARGO_HOME=%s PATH=%s/bin:$PATH\n' \
      "$RUSTUP_HOME" "$CARGO_HOME" "$CARGO_HOME" > /etc/profile.d/rust.sh \
 && ln -s "$CARGO_HOME"/bin/* /usr/local/bin/
ENV MAI_IN_CONTAINER=1
