# Builds the SDK from source with the adapter for the Flagsmith SDK behavioural spec.
# Build from the repository root: docker build -f spec-adapter/Dockerfile .
FROM rust:1.86-slim-bookworm AS build

WORKDIR /sdk
COPY Cargo.toml Cargo.lock README.md ./
COPY src ./src
COPY spec-adapter/Cargo.toml spec-adapter/Cargo.lock ./spec-adapter/
COPY spec-adapter/src ./spec-adapter/src
RUN cargo build --release --locked --manifest-path spec-adapter/Cargo.toml

FROM debian:bookworm-slim
# The SDK's HTTP client fails to build without system CA certificates.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /sdk/spec-adapter/target/release/flagsmith-spec-adapter /usr/local/bin/

ENV PORT=8080
EXPOSE 8080
CMD ["flagsmith-spec-adapter"]
