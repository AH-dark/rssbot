FROM rust:1-bullseye AS builder

ARG BIN

WORKDIR /usr/src/rssbot
COPY . .

RUN cargo build --release --bin ${BIN}

FROM debian:bullseye-slim AS runtime

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

ARG BIN

COPY --from=builder /usr/src/rssbot/target/release/${BIN} /usr/local/bin/rssbot

EXPOSE 8080

ENTRYPOINT /usr/local/bin/rssbot
