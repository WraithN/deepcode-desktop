FROM alpine:3.21

RUN apk --no-cache add ca-certificates

WORKDIR /app

COPY target/release/dh-gatewayd .

EXPOSE 2345 2346

CMD ["./dh-gatewayd", "--port", "2345", "--admin-port", "2346"]
