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

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libgcc-s1 mawk \
    && rm -rf /var/lib/apt/lists/* \
    && install -d -o 10001 -g 10001 -m 0755 /bootstrap \
    && install -d -o 10001 -g 10001 -m 0700 /run/tresleches
COPY --from=controller-build /out/controller /out/routerctl /usr/local/bin/
COPY --from=router-build /src/target/release/routerd /usr/local/bin/
COPY --chmod=755 scripts/docker-node.sh /usr/local/bin/docker-node
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/docker-node"]
