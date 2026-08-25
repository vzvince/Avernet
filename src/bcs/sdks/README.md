# Provider gRPC SDK Demo

This directory contains two inheritable Provider server SDK demonstrations.
Both implementations serve the same unary gRPC contract, and the standalone
Rust client can call either one by endpoint.

The canonical contract is
`src/bcs/api-contracts/provider-demo/v1/provider_demo.proto`.

## Run the Python server

From the Avernet repository root:

```bash
uv run --project src/bcs/sdks/python --extra test \
  python src/bcs/sdks/python/examples/echo_server.py --port 50051
```

Python integrations subclass `ProviderService`, implement the asynchronous
`invoke(message)` method and the `implementation` property, then pass the
instance to `ProviderServer`. See [python/README.md](python/README.md).

## Run the Java server

In another terminal:

```bash
mvn -f src/bcs/sdks/java/pom.xml compile exec:java \
  -Dexec.mainClass=com.avernet.bcs.provider.sdk.example.EchoServer \
  -Dexec.args='--port 50052'
```

Java integrations extend `ProviderService`, implement `invoke(String)` and
`implementation()`, then pass the instance to `ProviderGrpcServer`. The public
SDK uses standard gRPC and Protobuf dependencies; SOFA is not required. See
[java/README.md](java/README.md).

## Call either server with Rust

Point the standalone client at the Python port above, or change the endpoint
to port `50052` for Java:

```bash
cargo run --manifest-path src/bcs/Cargo.toml \
  --package bcs-provider-demo-client -- \
  --endpoint http://127.0.0.1:50051 \
  --message hello
```

The Python example returns:

```json
{"message":"python: hello","implementation":"python"}
```

Run both cross-language paths automatically with:

```bash
bash src/bcs/scripts/test_provider_grpc_sdk_demo.sh
```

## Scope

This is a transport and SDK-extension proof of concept. It does not replace or
change the current Provider SSE contract and does not yet define Provider event
semantics, bidirectional streaming, run lifecycle, HITL, service discovery,
registration, authentication, TLS, retries, load balancing, or health checks.
The Rust client is a standalone local executable and is not exposed through a
BCS HTTP route.
