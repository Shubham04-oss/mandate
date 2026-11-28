# syntax=docker/dockerfile:1.7
# Build from the repo root: docker build -f core/Dockerfile [--target migrate] .
FROM rust:1.99-trixie@sha256:8a4e90c7946b74626488b1446f712c0fe0ed1a596cbd3d871684bc9c5433ae07 AS build
WORKDIR /src
# Lets a shared build machine cap parallelism (--build-arg CARGO_BUILD_JOBS=2).
ARG CARGO_BUILD_JOBS
COPY Cargo.toml Cargo.lock ./
COPY core/Cargo.toml core/
# Dependencies first, from stub sources, in their own layer: a source-only change reuses it from the
# build cache (deploy.yml keeps it in the GitHub Actions cache). `target` stays in the layer on purpose.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    mkdir core/src && echo 'fn main() {}' > core/src/main.rs && touch core/src/lib.rs \
 && cargo build --release --locked -p mandate && rm -r core/src
COPY core core
# Newer mtimes than the stubs, so cargo rebuilds the crate itself.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    touch core/src/main.rs core/src/lib.rs \
 && cargo build --release --locked -p mandate && cp target/release/mandate /mandate

# The email templates, built to HTML and text with {{placeholders}} the core fills in (emails/README.md).
# The same pinned image as eve's (agents/Dockerfile).
FROM node:24-trixie-slim@sha256:4d00acb4bcc42c454c4eb801526f7ad7f772ea3bd4fd168c5cc2e384abcbb9d4 AS emails
WORKDIR /emails
COPY emails/package.json emails/package-lock.json ./
RUN npm ci --ignore-scripts
COPY emails/tsconfig.json ./
COPY emails/src src
RUN npm run build

FROM debian:trixie-slim@sha256:8aa5a7bf5a1955bddbe7d74fcdd0b91ab30e74362a8b73af672b0481b35b44a0 AS serve
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin mandate \
 && install -d -o mandate -m 0700 /run/mandate
COPY --from=build /mandate /usr/local/bin/mandate
COPY policies /app/policies
COPY --from=emails /emails/dist /app/emails
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
# The consumers verify mandate.core events with its public key (MANDATE_EVENT_PUBLIC_KEYS).
if [ -n "${MANDATE_EVENT_PUBLIC_KEY_PEM:-}" ]; then
  printf '%s\n' "$MANDATE_EVENT_PUBLIC_KEY_PEM" > /run/mandate/event-public-key.pem
  export MANDATE_EVENT_PUBLIC_KEYS="mandate.core=/run/mandate/event-public-key.pem"
fi
unset MANDATE_EVENT_PUBLIC_KEY_PEM
# The RDS root CAs that DATABASE_URL's sslrootcert names (sslmode=verify-full).
if [ -n "${RDS_CA_PEM:-}" ]; then
  printf '%s\n' "$RDS_CA_PEM" > /run/mandate/rds-ca.pem
fi
unset RDS_CA_PEM
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
CMD ["sh", "-c", "if [ -n \"${RDS_CA_PEM:-}\" ]; then printf '%s\\n' \"$RDS_CA_PEM\" > /run/mandate/rds-ca.pem; fi && mandate migrate && psql \"$MANDATE_MIGRATION_DATABASE_URL\" --single-transaction -v ON_ERROR_STOP=1 -v role=mandate_app -f /app/sql/app_role.sql"]

# The default target is the service image.
FROM serve
