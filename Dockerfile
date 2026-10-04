FROM public.ecr.aws/amazonlinux/amazonlinux:2023 AS builder

RUN dnf install -y \
    clang \
    gcc \
    make \
    protobuf-compiler \
    tar \
    xz \
 && dnf clean all

ENV RUSTUP_HOME="/usr/local/rustup"
ENV CARGO_HOME="/usr/local/cargo"
ENV PATH="/usr/local/cargo/bin:${PATH}"

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
 && rustup default stable

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY proto ./proto
COPY runner ./runner

RUN cargo build --release --package build-agent


FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal

ARG TARGETARCH

RUN dnf install -y \
    clang \
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

RUN case "${TARGETARCH}" in \
      amd64) wasmtime_arch="x86_64" ;; \
      arm64) wasmtime_arch="aarch64" ;; \
      *) echo "unsupported architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac \
 && curl -LO "https://github.com/bytecodealliance/wasmtime/releases/download/v43.0.1/wasmtime-v43.0.1-${wasmtime_arch}-linux.tar.xz" \
 && tar -xf "wasmtime-v43.0.1-${wasmtime_arch}-linux.tar.xz" \
 && mv "wasmtime-v43.0.1-${wasmtime_arch}-linux/wasmtime" /usr/local/bin/ \
 && rm -rf "wasmtime-v43.0.1-${wasmtime_arch}-linux"*

WORKDIR /app
COPY --from=builder /app/target/release/build-agent /usr/local/bin/build-agent
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY runner ./runner

RUN cargo build --manifest-path ./runner/Cargo.toml --target=wasm32-wasip1 --release

EXPOSE 8080

CMD ["build-agent"]
