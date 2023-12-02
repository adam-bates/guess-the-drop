
# Builder
FROM messense/rust-musl-cross:x86_64-musl as builder

WORKDIR /guess-the-drop
COPY . .

RUN apt-get update
RUN apt-get upgrade -y
RUN apt-get install -y musl-tools pkg-config libssl-dev

RUN cargo build --release --target=x86_64-unknown-linux-musl


# Lean Runner
FROM scratch

# Binary
COPY --from=builder /guess-the-drop/target/x86_64-unknown-linux-musl/release/guess-the-drop /guess-the-drop

# Secrets
COPY --from=builder /guess-the-drop/secrets /secrets

# Assets
COPY --from=builder /guess-the-drop/assets /assets
COPY --from=builder /guess-the-drop/migrations /migrations

# SSL Certs (for vendored openssl)
COPY --from=builder /etc/ssl/certs /usr/local/ssl/certs

ENTRYPOINT ["/guess-the-drop"]

