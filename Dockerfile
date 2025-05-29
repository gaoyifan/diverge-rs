FROM rust:alpine AS builder
RUN apk update
RUN apk add --no-cache musl-dev

WORKDIR /home/rust/diverge-rs/
ADD . .
RUN cargo build --release --target=$(uname -m)-unknown-linux-musl

FROM alpine
COPY --from=builder /home/rust/diverge-rs/target/*-unknown-linux-musl/release/cli /usr/local/bin/
COPY --from=builder /home/rust/diverge-rs/target/*-unknown-linux-musl/release/diverge /usr/local/bin/
ENTRYPOINT ["/usr/local/bin/diverge"] 