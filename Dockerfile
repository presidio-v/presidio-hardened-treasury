# Multi-stage for clean test image, but single for simplicity + full env.
# Use the exact pinned Rust version from rust-toolchain.toml
# Pinned by digest (Scorecard Pinned-Dependencies); Dependabot bumps the digest.
# Keep the tag in step with rust-toolchain.toml.
FROM rust:1.88.0-slim@sha256:38bc5a86d998772d4aec2348656ed21438d20fcdce2795b56ca434cf21430d89

# Install build essentials (for any native deps, though this workspace is mostly pure Rust)
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/treasury

# Copy manifests first for better layer caching (Cargo.toml + lock if present)
COPY Cargo.toml Cargo.lock rust-toolchain.toml deny.toml ./

# Copy all crates (source)
COPY crates ./crates

# Pre-build tests (compiles everything, caches deps)
RUN cargo test --workspace --no-run

# Default: full test run (workspace tests, including e2e golden_close and reconcile SLO harness with synthetic data)
# Use --test-threads=1 for deterministic output in container logs if desired, but default is fine.
CMD ["cargo", "test", "--workspace"]
