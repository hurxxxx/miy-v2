# This recipe is embedded in the worker. Uploaded Dockerfiles are never executed.
# Fetches have network access; application/package code runs in offline stages.
FROM rust:1.99.1-slim-bookworm@sha256:16113eb5d5c1e25d9359f40115225ce8719997fb0d9a3dab83e921da0308992f AS rust-tools
RUN rustup component add rustfmt clippy --toolchain 1.99.1

FROM rust-tools AS backend
WORKDIR /build
# The context has already passed the bounded source allowlist and bundle checks.
# Preserve integration tests, build.rs and assets used by include_bytes!/builds.
COPY . .
RUN cargo fetch --locked
RUN --network=none cargo fmt --all --check && cargo test --locked --offline && cargo build --release --locked --offline && cp target/release/miy-app /miy-app

FROM mcr.microsoft.com/playwright:v1.64.1-noble@sha256:7852dc5a7d72f5fe458a597d690f6c42fbc8e9e26ddfa95f8e6ef552558d9bbf AS node-tools
RUN npm install --global pnpm@10.34.5

FROM node-tools AS frontend
WORKDIR /build
ENV npm_config_manage_package_manager_versions=false npm_config_ignore_pnpmfile=true
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
RUN pnpm fetch --frozen-lockfile --ignore-scripts --ignore-pnpmfile
# Retain committed public/assets and other validated application build inputs.
COPY . .
RUN --network=none pnpm install --offline --frozen-lockfile && pnpm typecheck && pnpm build

FROM debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8
ARG MIY_APP_ID
ARG MIY_SOURCE_COMMIT
ARG MIY_SOURCE_HASH
ARG MIY_BUILD_ID
LABEL org.miy.app.id=$MIY_APP_ID org.opencontainers.image.revision=$MIY_SOURCE_COMMIT org.miy.source.archive-sha256=$MIY_SOURCE_HASH org.miy.build.id=$MIY_BUILD_ID
COPY --from=backend /miy-app /usr/local/bin/miy-app
COPY manifest.json /miy/manifest.json
COPY migrations /miy/migrations
COPY --from=frontend /build/dist/ui /miy/ui
COPY .miy-server-build.json /miy/build.json
USER 10001:10001
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/miy-app"]
