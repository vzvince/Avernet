# Java Provider SDK Demo

This module demonstrates a minimal inheritable gRPC Provider server. It uses
standard grpc-java and Protobuf dependencies only; SOFA is not required. This
is a transport proof of concept, not the final BCS Provider streaming SDK.

## Extend the SDK

```java
import com.avernet.bcs.provider.sdk.ProviderService;

public final class MyProvider extends ProviderService {
    @Override
    public String implementation() {
        return "my-java-provider";
    }

    @Override
    public String invoke(String message) {
        return "received: " + message;
    }
}
```

Pass the subclass instance to `ProviderGrpcServer` to host it with a standard
grpc-java Netty server. The module targets Java 8.

## Run the example

From the Avernet repository root:

```bash
mvn -f src/bcs/sdks/java/pom.xml compile exec:java \
  -Dexec.mainClass=com.avernet.bcs.provider.sdk.example.EchoServer \
  -Dexec.args='--port 50052'
```

Use `--port 0` to let the operating system select an available loopback port.

## Run the tests

```bash
mvn -f src/bcs/sdks/java/pom.xml test
```

The canonical Protobuf contract is compiled from
`src/bcs/api-contracts/provider-demo/v1/provider_demo.proto`; generated Java
sources remain Maven build output under `target/`.
