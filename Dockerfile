FROM node:22-slim AS frontend
WORKDIR /build/web-src
COPY web-src/package.json web-src/package-lock.json ./
RUN npm ci
COPY web-src/ ./
RUN npm run build

FROM rust:1.85-slim AS backend
WORKDIR /build
COPY Cargo.toml ./
COPY Cargo.lock* ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release || true
COPY src ./src
COPY migrations ./migrations
COPY --from=frontend /build/frontend/dist ./frontend/dist
COPY Cargo.toml ./Cargo.toml
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend /build/target/release/cloudmail /usr/local/bin/cloudmail
# 可通过 -c /data/config.toml 挂载自定义配置（含 database_dir=/data 等）
VOLUME ["/data"]
EXPOSE 8080
CMD ["cloudmail"]
