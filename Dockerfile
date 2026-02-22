FROM docker.io/library/rust:latest as builder
RUN apt-get update && apt-get install -y protobuf-compiler

WORKDIR /usr/src/app
COPY . .

RUN cargo install --path .

FROM gcr.io/distroless/cc-debian12

COPY --from=builder /usr/local/cargo/bin/depot_actor /usr/local/bin/depot_actor
COPY config ./config
ENV RUN_MODE=production

CMD ["depot_actor"]
