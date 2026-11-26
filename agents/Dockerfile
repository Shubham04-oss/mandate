# syntax=docker/dockerfile:1.7
# Build from agents/: docker build -f agents/Dockerfile agents
# eve needs Node 24 (agents/package.json engines).
FROM node:24-trixie-slim@sha256:4d00acb4bcc42c454c4eb801526f7ad7f772ea3bd4fd168c5cc2e384abcbb9d4 AS build
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
RUN npm run build && npm prune --omit=dev

FROM node:24-trixie-slim@sha256:4d00acb4bcc42c454c4eb801526f7ad7f772ea3bd4fd168c5cc2e384abcbb9d4
ENV NODE_ENV=production PORT=3000
WORKDIR /app
COPY --from=build --chown=root:root /app /app
# The image's unprivileged user; the app files stay root-owned and read-only to it.
USER node
EXPOSE 3000
# The config check fails fast on missing or invalid env, then the Postgres workflow world's
# migrations (idempotent, under a lock) run before eve starts. The RDS root CAs go to the file
# WORKFLOW_POSTGRES_URL's sslrootcert names (sslmode=verify-full).
CMD ["sh", "-c", "node agent/lib/env.ts && if [ -n \"${RDS_CA_PEM:-}\" ]; then printf '%s\\n' \"$RDS_CA_PEM\" > /tmp/rds-ca.pem; fi && node node_modules/@workflow/world-postgres/bin/setup.js && exec npm start"]
