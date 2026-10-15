FROM rust:1-bookworm AS builder

WORKDIR /app
COPY ./crates ./crates
COPY ./proto ./proto
COPY ./runner ./runner
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

RUN cargo build --release --package build-agent

FROM amazonlinux:2023

ARG TARGETARCH

RUN dnf install -y \
    ca-certificates \
    clang \
    curl-minimal \
    gcc \
    protobuf-compiler \
    tar \
    xz \
 && dnf clean all

ENV RUSTUP_HOME="/usr/local/rustup"
ENV CARGO_HOME="/usr/local/cargo"
ENV PATH="/usr/local/cargo/bin:${PATH}"

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
 && rustup default stable \
 && rustup target add wasm32-wasip1

RUN arch="${TARGETARCH:-$(uname -m)}" \
 && case "${arch}" in \
      amd64|x86_64) wasmtime_arch="x86_64" ;; \
      arm64|aarch64) wasmtime_arch="aarch64" ;; \
      *) echo "unsupported architecture: ${arch}" >&2; exit 1 ;; \
    esac \
 && curl -fsSL -o /tmp/wasmtime.tar.xz "https://github.com/bytecodealliance/wasmtime/releases/download/v43.0.1/wasmtime-v43.0.1-${wasmtime_arch}-linux.tar.xz" \
 && tar -xJf /tmp/wasmtime.tar.xz -C /tmp \
 && mv "/tmp/wasmtime-v43.0.1-${wasmtime_arch}-linux/wasmtime" /usr/local/bin/wasmtime \
 && rm -rf /tmp/wasmtime*

COPY --from=builder /app/target/release/build-agent /usr/local/bin/build-agent

ENV BUILD_AGENT_ADDRESS="0.0.0.0:8080"
EXPOSE 8080

CMD ["/usr/local/bin/build-agent"]
