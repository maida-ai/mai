# Disposable box for agent runs: sandbox bypass is safe here, not on your host.
# You don't run this directly: .agents/run builds the image on first use and
# re-executes itself inside it. Rebuild after editing: docker build -t mai-agents -f .agents/Dockerfile .agents
FROM rust:1.85-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends bubblewrap curl ca-certificates git python3 \
 && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
 && apt-get install -y --no-install-recommends nodejs \
 && npm install -g @openai/codex \
 && rm -rf /var/lib/apt/lists/*
RUN git config --system --add safe.directory '*'
# Agent harnesses may run commands in a login shell, where /etc/profile resets PATH and
# drops the Rust toolchain. Expose it both ways: profile.d and /usr/local/bin.
RUN printf 'export RUSTUP_HOME=%s CARGO_HOME=%s PATH=%s/bin:$PATH\n' \
      "$RUSTUP_HOME" "$CARGO_HOME" "$CARGO_HOME" > /etc/profile.d/rust.sh \
 && ln -s "$CARGO_HOME"/bin/* /usr/local/bin/
ENV MAI_IN_CONTAINER=1