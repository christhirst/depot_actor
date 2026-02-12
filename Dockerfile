FROM docker.io/library/rust:latest as builder

WORKDIR /usr/src/app
COPY . .

RUN cargo install --path .

FROM gcr.io/distroless/cc-debian12

COPY --from=builder /usr/local/cargo/bin/depot_actor /usr/local/bin/depot_actor
COPY config.toml .

CMD ["depot_actor"]
