# syntax=docker/dockerfile:1.7
# Build from the repo root: docker build -f core/Dockerfile [--target migrate] .
FROM rust:1.99-trixie AS build
WORKDIR /src
# Lets a shared build machine cap parallelism (--build-arg CARGO_BUILD_JOBS=2).
ARG CARGO_BUILD_JOBS
COPY Cargo.toml Cargo.lock ./
COPY core core
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p mandate && cp target/release/mandate /mandate

FROM debian:trixie-slim AS serve
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin mandate \
 && install -d -o mandate -m 0700 /run/mandate
COPY --from=build /mandate /usr/local/bin/mandate
COPY policies /app/policies
# ECS injects secrets as env vars but the core reads keys from files: write each PEM to a 0600
# file only this user can read, point the core at it, and drop the PEM from the environment.
COPY --chmod=0755 <<'SH' /usr/local/bin/entrypoint
#!/bin/sh
set -eu
umask 077
for var in MANDATE_ASSERTION_PRIVATE_KEY MANDATE_EVENT_SIGNING_KEY; do
  pem="$(printenv "${var}_PEM" || true)"
  if [ -n "$pem" ]; then
    printf '%s\n' "$pem" > "/run/mandate/$var.pem"
    export "$var=/run/mandate/$var.pem"
  fi
  unset "${var}_PEM"
done
exec /usr/local/bin/mandate "$@"
SH
USER 10001
ENV MANDATE_BIND=0.0.0.0:8080 MANDATE_POLICY_DIR=/app/policies
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/entrypoint"]
CMD ["serve"]

# One-off deploy step: migrate as the migration role, then re-apply the app role's grants so a
# new table is never left without them.
FROM serve AS migrate
USER root
RUN apt-get update \
 && apt-get install -y --no-install-recommends postgresql-client \
 && rm -rf /var/lib/apt/lists/*
COPY core/sql /app/sql
USER 10001
ENTRYPOINT []
CMD ["sh", "-c", "mandate migrate && psql \"$MANDATE_MIGRATION_DATABASE_URL\" -v ON_ERROR_STOP=1 -v role=mandate_app -f /app/sql/app_role.sql"]

# The default target is the service image.
FROM serve
