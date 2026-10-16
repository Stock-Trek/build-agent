ARG RUST_VERSION=1.99.0
ARG WASMTIME_VERSION=43.0.1


FROM public.ecr.aws/amazonlinux/amazonlinux:2023@sha256:18c71e6a20f8b0834b002b6002156a84eea76162d5adea72d2ea8b86800eaf91 AS builder

ARG RUST_VERSION

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN dnf install -y \
    gcc \
    make \
 && dnf clean all

ENV RUSTUP_HOME="/usr/local/rustup"
ENV CARGO_HOME="/usr/local/cargo"
ENV PATH="/usr/local/cargo/bin:${PATH}"

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain "${RUST_VERSION}" \
 && rustup target add wasm32-wasip1

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY proto ./proto
COPY runner ./runner

RUN cargo fetch --locked \
 && cargo build --release --package build-agent \
 && cargo build --release --package runner --bin build-runner \
 && cargo build --release --package runner --target wasm32-wasip1 --lib


FROM public.ecr.aws/amazonlinux/amazonlinux:2023@sha256:18c71e6a20f8b0834b002b6002156a84eea76162d5adea72d2ea8b86800eaf91 AS wasmtime

ARG WASMTIME_VERSION
ARG TARGETARCH

RUN dnf install -y \
    tar \
    xz \
 && dnf clean all

RUN arch="${TARGETARCH:-$(uname -m)}" \
 && case "${arch}" in \
      amd64|x86_64) wasmtime_arch="x86_64" ;; \
      arm64|aarch64) wasmtime_arch="aarch64" ;; \
      *) echo "unsupported architecture: ${arch}" >&2; exit 1 ;; \
    esac \
 && curl -fsSL -o /tmp/wasmtime.tar.xz \
      "https://github.com/bytecodealliance/wasmtime/releases/download/v${WASMTIME_VERSION}/wasmtime-v${WASMTIME_VERSION}-${wasmtime_arch}-linux.tar.xz" \
 && tar -xJf /tmp/wasmtime.tar.xz -C /tmp \
 && mv "/tmp/wasmtime-v${WASMTIME_VERSION}-${wasmtime_arch}-linux/wasmtime" /usr/local/bin/wasmtime \
 && rm -rf /tmp/wasmtime*


FROM public.ecr.aws/amazonlinux/amazonlinux:2023@sha256:18c71e6a20f8b0834b002b6002156a84eea76162d5adea72d2ea8b86800eaf91 AS runtime

ARG BUILD_UID=10001
ARG BUILD_GID=10001

RUN dnf install -y \
    gcc \
 && dnf clean all

ENV RUSTUP_HOME="/usr/local/rustup"
ENV CARGO_HOME="/usr/local/cargo"
ENV PATH="/usr/local/cargo/bin:${PATH}"
ENV HOME="/home/buildagent"
ENV BUILD_AGENT_ADDRESS="0.0.0.0:8080"

COPY --from=builder --chown=${BUILD_UID}:${BUILD_GID} /usr/local/rustup /usr/local/rustup
COPY --from=builder --chown=${BUILD_UID}:${BUILD_GID} /usr/local/cargo /usr/local/cargo
COPY --from=builder /app/target/release/build-agent /usr/local/bin/build-agent
COPY --from=builder /app/target/release/build-runner /usr/local/bin/build-runner
COPY --from=wasmtime /usr/local/bin/wasmtime /usr/local/bin/wasmtime

WORKDIR /app
COPY --chown=${BUILD_UID}:${BUILD_GID} Cargo.toml Cargo.lock ./
COPY --chown=${BUILD_UID}:${BUILD_GID} crates ./crates
COPY --chown=${BUILD_UID}:${BUILD_GID} proto ./proto
COPY --chown=${BUILD_UID}:${BUILD_GID} runner ./runner

RUN printf 'buildagent:x:%s:%s:build agent:%s:/sbin/nologin\n' "${BUILD_UID}" "${BUILD_GID}" "${HOME}" >> /etc/passwd \
 && printf 'buildagent:x:%s:\n' "${BUILD_GID}" >> /etc/group \
 && mkdir -p "${HOME}" \
 && chown "${BUILD_UID}:${BUILD_GID}" "${HOME}" /app

EXPOSE 8080
USER ${BUILD_UID}:${BUILD_GID}

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["curl", "-fsS", "http://127.0.0.1:8080/health"]

CMD ["/usr/local/bin/build-agent"]
