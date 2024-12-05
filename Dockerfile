FROM rust:1.83-slim as builder
WORKDIR /app
COPY . .
RUN cargo build --release

COPY --from=builder /app/target/release/aquila /usr/local/bin/
CMD ["aquila"]