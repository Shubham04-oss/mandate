# syntax=docker/dockerfile:1.7
# Build from agents/: docker build -f agents/Dockerfile agents
FROM node:22-trixie-slim AS build
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
RUN npm run build && npm prune --omit=dev

FROM node:22-trixie-slim
ENV NODE_ENV=production PORT=3000
WORKDIR /app
COPY --from=build --chown=root:root /app /app
# The image's unprivileged user; the app files stay root-owned and read-only to it.
USER node
EXPOSE 3000
CMD ["npm", "start"]
