FROM rust:1.99.1-slim-bookworm@sha256:16113eb5d5c1e25d9359f40115225ce8719997fb0d9a3dab83e921da0308992f AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/miy crates/miy
COPY templates/rust-react templates/rust-react
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/build/target cargo build --release --locked --no-default-features && cp target/release/miy /miy

FROM debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8
ARG MIY_SOURCE_COMMIT
LABEL org.miy.component="db-tunnel" org.opencontainers.image.revision=$MIY_SOURCE_COMMIT
COPY --from=build /miy /usr/local/bin/miy
USER 10001:10001
EXPOSE 5432
ENTRYPOINT ["/usr/local/bin/miy"]
CMD ["internal-db-tunnel"]
