FROM rust:1.99.1-slim-bookworm@sha256:16113eb5d5c1e25d9359f40115225ce8719997fb0d9a3dab83e921da0308992f AS backend
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY sdk/rust sdk/rust
COPY src src
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/build/target cargo build --release --locked && cp target/release/miy-app /miy-app

FROM debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8
ARG MIY_APP_ID
ARG MIY_SOURCE_COMMIT
LABEL org.miy.app.id=$MIY_APP_ID org.opencontainers.image.revision=$MIY_SOURCE_COMMIT
COPY --from=backend /miy-app /usr/local/bin/miy-app
COPY manifest.json /miy/manifest.json
COPY migrations /miy/migrations
COPY dist/ui /miy/ui
COPY dist/build.json /miy/build.json
USER 10001:10001
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/miy-app"]
