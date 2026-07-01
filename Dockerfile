# BudgetCut sync server — multi-stage build.
# Builds only the server crate (the desktop app is a separate workspace).
FROM rust:1-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock* ./
COPY crates ./crates
# Build the server in release. The desktop Tauri crate is NOT a workspace
# member, so this never pulls the GUI toolchain.
RUN cargo build --release -p budgetcut-server

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/budgetcut-server /usr/local/bin/budgetcut-server
ENV BUDGETCUT_BIND=0.0.0.0:8787
EXPOSE 8787
# Run as non-root.
RUN useradd -r -u 10001 budgetcut
USER budgetcut
ENTRYPOINT ["budgetcut-server"]
