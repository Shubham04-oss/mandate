# syntax=docker/dockerfile:1.7
# Build from agents/: docker build -f agents/Dockerfile agents
# eve needs Node 24 (agents/package.json engines).
FROM node:24-trixie-slim AS build
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
RUN npm run build && npm prune --omit=dev

FROM node:24-trixie-slim
ENV NODE_ENV=production PORT=3000
WORKDIR /app
COPY --from=build --chown=root:root /app /app
# The image's unprivileged user; the app files stay root-owned and read-only to it.
USER node
EXPOSE 3000
# The Postgres workflow world's migrations (idempotent, under a lock) run before eve starts.
CMD ["sh", "-c", "node node_modules/@workflow/world-postgres/bin/setup.js && exec npm start"]
