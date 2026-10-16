# harlo-relay in a container. Build from the repository root (it serves the
# phone app from crates/harlo-app/phone):
#
#   docker build -f crates/harlo-relay/Dockerfile -t harlo-relay .
#   docker run -p 8080:8080 harlo-relay
#
# Put HTTPS in front of it; add -e HARLO_RELAY_TRUST_FORWARDED=1 when only the
# proxy can reach it. To send the iOS app's notifications, give it the app's
# APNs key (readable by the container's user, 65532):
#
#   docker run -p 8080:8080 -v /path/AuthKey_ABCDE12345.p8:/key/AuthKey_ABCDE12345.p8:ro \
#     -e HARLO_RELAY_APNS_KEY=/key/AuthKey_ABCDE12345.p8 -e HARLO_RELAY_APNS_TEAM=<Team ID> harlo-relay
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p harlo-relay

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/harlo-relay /usr/local/bin/harlo-relay
ENV HARLO_RELAY_LISTEN=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/harlo-relay"]
