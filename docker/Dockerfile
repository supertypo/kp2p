##
# builder image
##
FROM rust:1.81-alpine AS builder

RUN apk --no-cache add \
  musl-dev \
  protobuf-dev \
  g++ \
  clang15-dev \
  linux-headers \
  wasm-pack \
  openssl-dev

RUN rustup component add rustfmt

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

RUN apk --no-cache add libstdc++

USER 13337:13337

COPY --from=builder /work/target/release/kp2p /usr/local/bin/

ARG VERSION
ARG COMMIT_ID

ENV VERSION=$VERSION \
  COMMIT_ID=$COMMIT_ID

ENTRYPOINT ["kp2p"]
CMD ["--help"]
