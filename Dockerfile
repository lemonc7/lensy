FROM rust:1.95-slim-trixie AS chef

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        curl \
        musl-tools \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown x86_64-unknown-linux-musl
ENV CC_x86_64_unknown_linux_musl=musl-gcc
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc
RUN cargo install cargo-chef


FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json


FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .

RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
RUN cargo binstall dioxus-cli@0.7.10 --root /.cargo -y --force
ENV PATH="/.cargo/bin:$PATH"
ENV SQLX_OFFLINE=true
RUN dx bundle --package lensy --web --release \
    @server --target x86_64-unknown-linux-musl
# scratch 没有动态链接器；构建阶段直接阻止误生成动态服务端。
RUN ! readelf -l dist/server | grep -q "Requesting program interpreter"

FROM scratch AS runtime

WORKDIR /app
ENV RUST_LOG=info,dioxus_server=warn,hyper_util=warn

COPY --from=builder --chown=1000:1000 /app/dist/ /app/

USER 1000:1000
EXPOSE 8080
ENTRYPOINT ["/app/server"]
