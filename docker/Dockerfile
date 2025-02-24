##
# builder image
##
FROM rust:1.83-alpine AS builder

RUN apk --no-cache add \
  musl-dev \
  protobuf-dev \
  g++ \
  clang15-dev \
  linux-headers \
  wasm-pack \
  openssl-dev

WORKDIR /work

ENV RUSTFLAGS="-C target-feature=-crt-static -C link-args=-s" \
  CARGO_REGISTRIES_CRATES_IO_PROTOCOL="sparse"

ARG REPO_DIR=.
COPY "$REPO_DIR" .

RUN cargo build --release


##
# runtime image
##
FROM alpine

ARG VERSION
ENV VERSION=$VERSION

RUN apk --no-cache add libstdc++

COPY --from=builder /work/target/release/kp2p /usr/local/bin/

USER 13337:13337

ENTRYPOINT ["kp2p"]
CMD ["--help"]
