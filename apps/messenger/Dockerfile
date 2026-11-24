FROM rust:1.99.1-slim-bookworm@sha256:16113eb5d5c1e25d9359f40115225ce8719997fb0d9a3dab83e921da0308992f AS backend
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY sdk sdk
COPY src src
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/build/target cargo build --release --locked && cp target/release/miy-app /miy-app

FROM debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8 AS rust-web-v1
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

FROM node:22-bookworm-slim@sha256:e9f565d32859f0cbfb52125eafe8be39390f24dac302dc3cdf566e8ba3ab21ee AS node-runtime
FROM rust-web-v1 AS rust-web-node-v1
COPY --from=node-runtime /usr/local/bin/node /usr/local/bin/node
COPY dist/server /miy/server

FROM rust-web-v1 AS default-app
