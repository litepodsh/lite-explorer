FROM oven/bun:latest AS deps
WORKDIR /app

COPY apps/web/package.json apps/web/bun.lock ./apps/web/
RUN cd apps/web && bun install

FROM oven/bun:latest AS builder
WORKDIR /app

COPY --from=deps /app/apps/web/node_modules ./apps/web/node_modules
COPY apps/web/ ./apps/web/
# The web app reads the desktop release version from ../../../desktop.
COPY apps/desktop/src-tauri/tauri.conf.json ./apps/desktop/src-tauri/tauri.conf.json
RUN cd apps/web && bun run build

FROM oven/bun:latest AS runner
WORKDIR /app

ENV HOST=0.0.0.0
ENV PORT=3000

COPY --from=builder /app/apps/web/dist ./dist

EXPOSE 3000
CMD ["bun", "./dist/server/entry.mjs"]
