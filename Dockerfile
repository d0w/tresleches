FROM golang:1.27.3-bookworm AS controller-build
WORKDIR /src/controller
COPY controller/go.mod controller/go.sum ./
RUN go mod download
COPY controller/ ./
RUN CGO_ENABLED=0 go build -trimpath -o /out/controller ./cmd/controller \
    && CGO_ENABLED=0 go build -trimpath -o /out/routerctl ./cmd/router

FROM rust:1.98.1-bookworm AS router-build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY mesh_router/ mesh_router/
COPY one_shot_test/ one_shot_test/
RUN cargo build --locked --release -p mesh_router --bin routerd

FROM debian:bookworm-slim AS node-base
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libgcc-s1 mawk \
    && rm -rf /var/lib/apt/lists/* \
    && install -d -o 10001 -g 10001 -m 0755 /bootstrap \
    && install -d -o 10001 -g 10001 -m 0700 /run/tresleches
COPY --from=controller-build /out/controller /out/routerctl /usr/local/bin/
COPY --chmod=755 scripts/docker-node.sh /usr/local/bin/docker-node
ENTRYPOINT ["/usr/local/bin/docker-node"]

# Build this target without compiling the legacy Rust userspace router.
FROM node-base AS tun
RUN apt-get update \
    && apt-get install -y --no-install-recommends iproute2 iputils-ping \
    && rm -rf /var/lib/apt/lists/* \
    && chown 0:0 /bootstrap /run/tresleches
ENV NODE_MODE=tun
USER 0:0

# Keep the original two-process image as the default build target.
FROM node-base AS legacy
COPY --from=router-build /src/target/release/routerd /usr/local/bin/
USER 10001:10001
