FROM ghcr.io/cargo-lambda/cargo-lambda:latest AS builder

WORKDIR /app
COPY ./src ./src
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

RUN cargo lambda build --release
RUN cp target/lambda/algorithm-compiler/bootstrap .





FROM public.ecr.aws/lambda/provided:al2023

RUN dnf install -y \
    clang \
    gcc \
    git \
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

RUN curl -LO https://github.com/bytecodealliance/wasmtime/releases/download/v43.0.1/wasmtime-v43.0.1-x86_64-linux.tar.xz \
 && tar -xf wasmtime-v43.0.1-x86_64-linux.tar.xz \
 && mv wasmtime-v43.0.1-x86_64-linux/wasmtime /usr/local/bin/ \
 && rm -rf wasmtime-v43.0.1-x86_64-linux*

WORKDIR ${LAMBDA_RUNTIME_DIR}
COPY --from=builder /app/bootstrap .
COPY ./algorithm-runner ./algorithm-runner

RUN cargo build --manifest-path ./algorithm-runner/Cargo.toml --target=wasm32-wasip1 --release

CMD ["bootstrap"]
