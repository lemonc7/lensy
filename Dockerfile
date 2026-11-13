FROM rust:1.100-slim-trixie AS builder

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential ca-certificates musl-tools \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add x86_64-unknown-linux-musl \
    && cargo install topcoat-cli --version 0.12.1 --locked

ENV SQLX_OFFLINE=true \
    CARGO_BUILD_TARGET=x86_64-unknown-linux-musl \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc \
    CC_x86_64_unknown_linux_musl=musl-gcc

COPY . .
RUN topcoat asset bundle --release --out /app/bundled-assets

FROM alpine:3.24 AS runtime
WORKDIR /app
RUN apk add --no-cache ca-certificates \
    && addgroup -g 1000 lensy \
    && adduser -D -H -u 1000 -G lensy lensy \
    && mkdir -p /app/data /app/config \
    && chown lensy:lensy /app/data /app/config

ENV RUST_LOG=info,hyper_util=warn
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/lensy ./lensy
COPY --from=builder /app/bundled-assets ./assets
USER lensy
EXPOSE 8080
ENTRYPOINT ["/app/lensy"]
