FROM rust:1.55 AS builder
WORKDIR /lnd-exporter
COPY Cargo.lock . 
COPY Cargo.toml .
COPY src ./src
COPY lnrpc ./lnrpc
RUN rustup component add rustfmt
RUN cargo build --release

FROM debian:buster-slim
RUN apt-get update && apt-get install -y libssl-dev 
COPY --from=builder lnd-exporter/target/release/lnd-exporter ./lnd-exporter

CMD ["./lnd-exporter"]
