# Provider gRPC SDK Demo Design

## Goal

Build the smallest cross-language proof of concept that demonstrates all of
the following without changing the BCS runtime:

- a Python application can subclass a public SDK service and receive a gRPC
  call;
- a Java application can subclass a public SDK service that depends only on
  standard gRPC and Protobuf libraries;
- one standalone Rust executable can call either implementation by endpoint;
- Python, Java, and Rust use the same checked-in Protobuf contract.

The proof of concept deliberately excludes SSE event semantics, bidirectional
streaming, SOFA dependencies, service discovery, authentication, TLS, retry,
and production registration.

## Options Considered

### 1. Standalone Rust command-line client — selected

The user starts either demo server and invokes a local Rust executable with an
endpoint and message. This validates the wire protocol and SDK extension point
without exposing a network-access proxy or modifying BCS composition.

### 2. Standalone HTTP-to-gRPC gateway

This would make remote testing convenient, but accepting arbitrary target
hosts creates an SSRF and port-scanning surface that would require admission,
authentication, and target allow-listing. Those concerns do not help validate
the SDK.

### 3. BCS HTTP endpoint

This is closest to a future product integration, but it introduces new BCS
Application/Port/Adapter wiring before the transport proof is complete. It is
deferred until the standalone interoperability test passes.

## Repository Layout

```text
src/bcs/
├── api-contracts/provider-demo/v1/provider_demo.proto
├── sdks/
│   ├── python/
│   │   ├── pyproject.toml
│   │   ├── src/bcs_provider_sdk/
│   │   ├── tests/
│   │   └── examples/echo_server.py
│   └── java/
│       ├── pom.xml
│       ├── src/main/java/
│       ├── src/test/java/
│       └── examples/
└── crates/tools/bcs-provider-demo-client/
    ├── Cargo.toml
    ├── build.rs
    └── src/main.rs
```

The Protobuf file is the authority for the cross-language wire contract.
Language-specific generated sources or compiled classes are build products of
that contract and must not redefine its semantics.

## Protocol

The demo uses one unary operation:

```proto
syntax = "proto3";

package bcs.provider.demo.v1;

service ProviderDemo {
  rpc Invoke(InvokeRequest) returns (InvokeResponse);
}

message InvokeRequest {
  string message = 1;
}

message InvokeResponse {
  string message = 1;
  string implementation = 2;
}
```

Unary RPC is sufficient for the proof: if both SDKs receive this call through
an inherited handler, the same generated transport foundation can later add a
versioned bidirectional `Run` method.

## Python SDK

The public extension point is an abstract `ProviderService` with an async
`invoke(message)` method and an implementation name. An internal generated
gRPC servicer adapter delegates incoming RPCs to that object. `ProviderServer`
owns `grpc.aio.Server` lifecycle and accepts a host and port.

Generated Python Protobuf and gRPC modules are packaged with the SDK so SDK
consumers need only runtime dependencies. `grpcio-tools` is a development-time
generation dependency, not a consumer requirement.

The example subclasses `ProviderService` and returns `python: <message>`.

## Java SDK

The public extension point is an abstract `ProviderService` class. A package-
private adapter extends the standard generated
`ProviderDemoGrpc.ProviderDemoImplBase`, delegates to the user's subclass, and
returns the result through `StreamObserver`.

`ProviderGrpcServer` starts a standard grpc-java Netty server and is
`AutoCloseable`. The SDK has no SOFA compiler, SOFA starter, Triple binding, or
private artifact dependency. It targets Java 8 because that is the Java
runtime available in the current development environment.

The example subclasses `ProviderService` and returns `java: <message>`.

## Rust Client

`bcs-provider-demo-client` is a standalone Cargo binary. It is a BCS workspace
tool only for build and dependency management; no BCS server crate depends on
it and it is not mounted in the BCS HTTP router.

Example invocation:

```bash
cargo run -p bcs-provider-demo-client -- \
  --endpoint http://127.0.0.1:50051 \
  --message hello
```

The client uses `tonic`, applies a short request timeout, and prints one JSON
object containing `message` and `implementation`. Connection and RPC failures
are written to stderr and produce a non-zero process exit.

The target is supplied by the local command invoker, so the PoC does not add a
remotely exploitable arbitrary-target HTTP API.

## Data Flow

```text
Rust CLI
  -> standard gRPC/HTTP2 InvokeRequest
  -> Python grpc.aio OR Java grpc-netty server
  -> generated servicer adapter
  -> user-defined ProviderService subclass
  -> InvokeResponse
  -> Rust CLI JSON output
```

## Error Handling

- An SDK handler exception becomes gRPC `INTERNAL` and does not expose a stack
  trace in the wire response.
- An invalid endpoint is rejected by the Rust CLI before invocation.
- Connection refusal, deadline expiry, and non-OK gRPC status result in a
  non-zero client exit.
- Server shutdown is explicit and idempotent enough for examples and tests.

Production TLS, authentication, retries, load balancing, health checking, and
SOFA/MOSN metadata remain outside this demo.

## Testing

Each language follows test-first implementation:

1. Python unit tests prove a subclass receives the message and its result is
   returned through the real generated servicer adapter.
2. Java unit tests prove a subclass receives the message and the standard
   generated gRPC adapter returns its result.
3. Rust tests prove CLI argument validation and response JSON projection.
4. End-to-end smoke tests start the Python server and Java server in turn and
   call each with the same Rust executable.
5. Existing `bcs-protocol` tests remain green, demonstrating that the PoC does
   not change the current SSE/HTTP Provider contract.

## Future Evolution

After this proof passes, a separate protocol design may add a versioned
bidirectional `Run` RPC and map the existing Provider event model into typed or
JSON-bearing Protobuf envelopes. SOFA support, if required, remains an optional
adapter rather than a dependency of the public Java SDK.
