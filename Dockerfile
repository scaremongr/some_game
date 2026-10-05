FROM rust:1.90 AS build
WORKDIR /build
RUN rustup target add wasm32-unknown-unknown
COPY Cargo.toml Cargo.lock ./
COPY .cargo .cargo
COPY src src
COPY combat/Cargo.toml combat/Cargo.lock ./combat/
COPY combat/src combat/src
RUN cargo build --locked --release --target wasm32-unknown-unknown --bin some_game && \
    cargo build --locked --release --target wasm32-unknown-unknown --manifest-path combat/Cargo.toml

FROM node:24-alpine
WORKDIR /app
ENV NODE_ENV=production HOST=0.0.0.0 PORT=8080
COPY package.json package-lock.json ./
RUN npm ci --omit=dev --ignore-scripts && npm cache clean --force
COPY server server
COPY web/combat.js web/combat.js
COPY web/index.html web/gl.js web/audio.js web/pose.js web/bridge.js web/arena.js web/arena.css web/combat.js web/predict.js web/scenery.js web/sound.js ./dist/
COPY assets/ ./dist/assets/
COPY --from=build /build/target/wasm32-unknown-unknown/release/some_game.wasm ./dist/
COPY --from=build /build/combat/target/wasm32-unknown-unknown/release/arena_combat.wasm ./dist/
USER node
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD node -e "fetch('http://127.0.0.1:8080/health').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))"
CMD ["node", "server/index.mjs"]
