# Runs this SDK, built from source, under the Flagsmith SDK behavioural spec.
# docker build -f sdk-spec/Dockerfile --build-context harness=<sdk-spec>/harness/rust .
FROM docker.io/library/rust:1.86-slim-bookworm AS build

WORKDIR /sdk
COPY Cargo.toml Cargo.lock README.md ./
COPY src ./src
COPY sdk-spec/Cargo.toml sdk-spec/steps.rs ./sdk-spec/

# The harness builds the steps file in, from the SDK checkout at /sdk.
COPY --from=harness . /harness
# Resolve dependencies to the versions the SDK is tested with.
COPY Cargo.lock /harness/Cargo.lock
RUN cargo build --release --manifest-path /harness/Cargo.toml

FROM docker.io/library/debian:bookworm-slim
# The SDK's HTTP client fails to build without system CA certificates.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /harness/target/release/sdk-spec-harness /usr/local/bin/

ENTRYPOINT ["sdk-spec-harness"]
