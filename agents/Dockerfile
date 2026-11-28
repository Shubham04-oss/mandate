# syntax=docker/dockerfile:1.7
# Build from agents/: docker build -f agents/Dockerfile agents
# eve runs on Node 26 LTS; CI's agents job uses the same major.
FROM node:26-trixie-slim@sha256:abe7e5a82b61a8ef67b315b85497c9e65173387ccf2f4a029248184b8e020be5 AS build
# eve's CLI sends usage data to Vercel; off in the build and at run time (owner's call, ORY-67).
ENV EVE_TELEMETRY_DISABLED=1
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
RUN npm run build && npm prune --omit=dev

FROM node:26-trixie-slim@sha256:abe7e5a82b61a8ef67b315b85497c9e65173387ccf2f4a029248184b8e020be5
ENV NODE_ENV=production PORT=3000 EVE_TELEMETRY_DISABLED=1
WORKDIR /app
COPY --from=build --chown=root:root /app /app
# The image's unprivileged user; the app files stay root-owned and read-only to it.
USER node
EXPOSE 3000
# The config check fails fast on missing or invalid env, then the Postgres workflow world's
# migrations (idempotent, under a lock) run before eve starts. The RDS root CAs go to the file
# WORKFLOW_POSTGRES_URL's sslrootcert names (sslmode=verify-full).
CMD ["sh", "-c", "node agent/lib/env.ts && if [ -n \"${RDS_CA_PEM:-}\" ]; then printf '%s\\n' \"$RDS_CA_PEM\" > /tmp/rds-ca.pem; fi && node node_modules/@workflow/world-postgres/bin/setup.js && exec npm start"]
