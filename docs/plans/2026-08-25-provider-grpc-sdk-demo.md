# Provider gRPC SDK Demo Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build inheritable Python and Java gRPC server SDK demos plus one standalone Rust client that calls either implementation through a shared unary Protobuf contract.

**Architecture:** A checked-in Proto under `src/bcs/api-contracts` is the only wire-contract authority. Python and Java SDKs adapt generated gRPC server bases to small user-subclass extension points, while a Rust workspace tool generates only the client and never enters the BCS server dependency graph.

**Tech Stack:** Protobuf, Python 3.10+ with `grpcio`, Java 8 with grpc-java/Maven, Rust 1.91 with `tonic`, `prost`, `clap`, and vendored protoc.

---

### Task 1: Add the canonical demo contract

**Files:**
- Create: `src/bcs/api-contracts/provider-demo/v1/provider_demo.proto`
- Create: `src/bcs/tests/provider_grpc_sdk_demo/test_proto_contract.py`

**Step 1: Write the failing contract test**

Create a test that loads the Proto text and asserts the package, unary service,
request field, and response fields:

```python
from pathlib import Path


PROTO = (
    Path(__file__).parents[2]
    / "api-contracts/provider-demo/v1/provider_demo.proto"
)


def test_provider_demo_proto_locks_unary_interop_surface() -> None:
    text = PROTO.read_text(encoding="utf-8")
    assert "package bcs.provider.demo.v1;" in text
    assert "rpc Invoke(InvokeRequest) returns (InvokeResponse);" in text
    assert "string message = 1;" in text
    assert "string implementation = 2;" in text
```

**Step 2: Run the test and verify RED**

Run:

```bash
uv run --with pytest pytest src/bcs/tests/provider_grpc_sdk_demo/test_proto_contract.py -q
```

Expected: FAIL because `provider_demo.proto` does not exist.

**Step 3: Add the minimal Proto**

Add the approved `ProviderDemo.Invoke` contract with `InvokeRequest.message`
and `InvokeResponse.message/implementation`. Add `java_multiple_files`,
`java_package`, and `java_outer_classname` options without adding transport or
SOFA-specific options.

**Step 4: Verify GREEN and compile the Proto**

Run:

```bash
uv run --with pytest pytest src/bcs/tests/provider_grpc_sdk_demo/test_proto_contract.py -q
protoc --proto_path=src/bcs/api-contracts/provider-demo/v1 \
  --descriptor_set_out=/tmp/provider-demo.pb \
  src/bcs/api-contracts/provider-demo/v1/provider_demo.proto
```

Expected: one passing test and a successful `protoc` exit.

**Step 5: Commit**

```bash
git add src/bcs/api-contracts/provider-demo/v1/provider_demo.proto \
  src/bcs/tests/provider_grpc_sdk_demo/test_proto_contract.py
git commit -m "feat(bcs): define provider gRPC demo contract"
```

### Task 2: Build the Python inheritable SDK

**Files:**
- Create: `src/bcs/sdks/python/pyproject.toml`
- Create: `src/bcs/sdks/python/scripts/generate_proto.sh`
- Create: `src/bcs/sdks/python/src/bcs_provider_sdk/__init__.py`
- Create: `src/bcs/sdks/python/src/bcs_provider_sdk/service.py`
- Create: `src/bcs/sdks/python/src/bcs_provider_sdk/server.py`
- Generate: `src/bcs/sdks/python/src/bcs_provider_sdk/_generated/provider_demo_pb2.py`
- Generate: `src/bcs/sdks/python/src/bcs_provider_sdk/_generated/provider_demo_pb2_grpc.py`
- Create: `src/bcs/sdks/python/tests/test_server.py`
- Create: `src/bcs/sdks/python/examples/echo_server.py`
- Create: `src/bcs/sdks/python/README.md`

**Step 1: Add packaging and deterministic Proto generation**

Use a standard `src`-layout package named `bcs-provider-sdk-demo`. Runtime
dependencies are `grpcio` and `protobuf`; test/generation dependencies are
`pytest`, `pytest-asyncio`, and `grpcio-tools`. The generation script copies
the canonical Proto into a temporary path matching
`bcs_provider_sdk/_generated`, invokes `grpc_tools.protoc`, and removes the
temporary source copy. Generated modules are committed so consumers do not
need `grpcio-tools`.

Run the generator and verify both generated modules import:

```bash
uv run --project src/bcs/sdks/python --extra test \
  src/bcs/sdks/python/scripts/generate_proto.sh
uv run --project src/bcs/sdks/python --extra test python -c \
  'from bcs_provider_sdk._generated import provider_demo_pb2, provider_demo_pb2_grpc'
```

Expected: both commands exit successfully. Generated code is exempt from the
test-first rule; all handwritten runtime behavior remains test-first.

**Step 2: Write the failing subclass round-trip test**

The test defines:

```python
class EchoProvider(ProviderService):
    @property
    def implementation(self) -> str:
        return "python"

    async def invoke(self, message: str) -> str:
        return f"python: {message}"
```

It starts `ProviderServer(EchoProvider(), host="127.0.0.1", port=0)`, opens a
real `grpc.aio.insecure_channel`, calls the generated `Invoke`, and asserts the
message, implementation, and that the chosen bound port is non-zero.

**Step 3: Run the Python test and verify RED**

Run:

```bash
uv run --project src/bcs/sdks/python --extra test pytest \
  src/bcs/sdks/python/tests/test_server.py -q
```

Expected: FAIL because `ProviderService` and `ProviderServer` do not exist.

**Step 4: Implement the minimal SDK**

Implement:

```python
class ProviderService(ABC):
    @property
    @abstractmethod
    def implementation(self) -> str: ...

    @abstractmethod
    async def invoke(self, message: str) -> str: ...
```

`ProviderServer.start()` registers an internal generated servicer, calls
`add_insecure_port`, starts the `grpc.aio.Server`, and exposes `bound_port`.
`stop()` stops it. The adapter awaits the user subclass and constructs the
generated response.

**Step 5: Verify GREEN**

Run the test from Step 3. Expected: PASS with no warnings.

**Step 6: Add a failing handler-error test**

Add a subclass that raises `RuntimeError("secret detail")`. Assert the gRPC
status is `INTERNAL` and the wire details do not contain `secret detail`.

Run the test and confirm it fails before adding error translation. Then catch
handler exceptions in the adapter, log locally, and call
`context.abort(grpc.StatusCode.INTERNAL, "provider invocation failed")`.

Re-run the suite. Expected: both tests PASS.

**Step 7: Add and manually smoke the example**

Add `examples/echo_server.py` with an `EchoProvider` subclass and argparse
host/port flags. Document:

```bash
uv run --project src/bcs/sdks/python --extra test \
  python src/bcs/sdks/python/examples/echo_server.py --port 50051
```

Only start the process briefly here; the Rust client integration arrives in
Task 4.

**Step 8: Commit**

```bash
git add src/bcs/sdks/python
git commit -m "feat(bcs): add inheritable Python gRPC provider demo SDK"
```

### Task 3: Build the Java inheritable SDK

**Files:**
- Create: `src/bcs/sdks/java/pom.xml`
- Create: `src/bcs/sdks/java/src/main/java/com/avernet/bcs/provider/sdk/ProviderService.java`
- Create: `src/bcs/sdks/java/src/main/java/com/avernet/bcs/provider/sdk/ProviderGrpcServer.java`
- Create: `src/bcs/sdks/java/src/main/java/com/avernet/bcs/provider/sdk/example/EchoServer.java`
- Create: `src/bcs/sdks/java/src/test/java/com/avernet/bcs/provider/sdk/ProviderGrpcServerTest.java`
- Create: `src/bcs/sdks/java/README.md`

**Step 1: Add a public-only Maven build**

Configure Java 8 and standard Maven Central dependencies only:

- `protobuf-java`
- `grpc-protobuf`
- `grpc-stub`
- `grpc-netty-shaded`
- `javax.annotation-api`
- JUnit Jupiter for tests

Use `protobuf-maven-plugin`, `protoc-gen-grpc-java`, and `os-maven-plugin` to
compile the canonical Proto from
`${project.basedir}/../../api-contracts/provider-demo/v1`. Do not add any SOFA
artifact, repository, compiler, annotation, or XML.

Run:

```bash
mvn -f src/bcs/sdks/java/pom.xml generate-sources
```

Expected: generated standard grpc-java sources under `target/`.

**Step 2: Write the failing Java round-trip test**

Define an anonymous `ProviderService` subclass that returns
`java: <message>`. Start `ProviderGrpcServer` on loopback port zero, connect
with `ManagedChannelBuilder.usePlaintext()`, call the generated blocking stub,
and assert `message`, `implementation`, and a non-zero bound port.

**Step 3: Run the Java test and verify RED**

Run:

```bash
mvn -f src/bcs/sdks/java/pom.xml \
  -Dtest=ProviderGrpcServerTest#subclassReceivesGrpcInvocation test
```

Expected: compilation FAIL because the SDK classes do not exist.

**Step 4: Implement the minimal Java SDK**

`ProviderService` contains:

```java
public abstract String implementation();
public abstract String invoke(String message) throws Exception;
```

`ProviderGrpcServer` owns a standard grpc-java `Server`, provides
`start()`, `getPort()`, `blockUntilShutdown()`, and idempotent `close()`, and
uses a private generated-service adapter to call the subclass.

**Step 5: Verify GREEN**

Run the test from Step 3. Expected: PASS.

**Step 6: Add the failing handler-error test**

Use a subclass whose `invoke` throws `RuntimeException("secret detail")`.
Assert `StatusRuntimeException` has code `INTERNAL` and public description
`provider invocation failed`. Confirm RED, implement the minimal status
translation, and confirm GREEN.

**Step 7: Add the executable example**

Add `EchoServer` with `--host`/`--port` parsing limited to the demo needs.
Document execution through `exec-maven-plugin`:

```bash
mvn -f src/bcs/sdks/java/pom.xml compile exec:java \
  -Dexec.mainClass=com.avernet.bcs.provider.sdk.example.EchoServer \
  -Dexec.args='--port 50052'
```

**Step 8: Verify the Java artifact has no SOFA dependencies**

Run:

```bash
mvn -f src/bcs/sdks/java/pom.xml dependency:tree
```

Expected: no dependency whose group or artifact contains `sofa`.

**Step 9: Commit**

```bash
git add src/bcs/sdks/java
git commit -m "feat(bcs): add inheritable Java gRPC provider demo SDK"
```

### Task 4: Build the standalone Rust client

**Files:**
- Modify: `src/bcs/Cargo.toml`
- Modify: `src/bcs/Cargo.lock`
- Create: `src/bcs/crates/tools/bcs-provider-demo-client/Cargo.toml`
- Create: `src/bcs/crates/tools/bcs-provider-demo-client/build.rs`
- Create: `src/bcs/crates/tools/bcs-provider-demo-client/src/lib.rs`
- Create: `src/bcs/crates/tools/bcs-provider-demo-client/src/main.rs`

**Step 1: Write failing library tests**

Tests exercise the wished-for public helpers:

- endpoint parsing rejects a value without `http://` or `https://`;
- a generated `InvokeResponse` renders as exactly one JSON object with
  `message` and `implementation`.

**Step 2: Add the crate to the workspace and verify RED**

Add only the member and test dependencies needed for the tests, then run:

```bash
cargo test -p bcs-provider-demo-client
```

Expected: FAIL because parsing/rendering functions are absent.

**Step 3: Implement parsing and JSON rendering**

Use `url::Url` for endpoints and `serde_json::to_string` for deterministic
output. Return errors instead of panicking; workspace lints deny `unwrap` and
`expect`.

Run the Task 2 command. Expected: PASS.

**Step 4: Add a failing real-client test**

In the test, start an in-process tonic TCP server on an ephemeral loopback
port using the generated server trait, invoke it through the client function,
and assert the returned generated response. This proves the CLI library uses a
real HTTP/2 gRPC channel rather than only formatting data.

Run the test and confirm RED because `invoke` is absent.

**Step 5: Implement the tonic client**

Use vendored protoc from `build.rs`, generate the client and server for tests,
connect through `tonic::transport::Endpoint`, apply a five-second timeout, and
send `InvokeRequest`. Map errors into `anyhow::Result` with endpoint context.

Re-run the tests. Expected: PASS.

**Step 6: Implement the thin CLI binary**

Use `clap`:

```text
--endpoint <http://host:port>
--message <text>
```

Call the tested library, print response JSON on stdout, print an error through
the normal Rust main error path, and return non-zero on failure.

Run:

```bash
cargo run -p bcs-provider-demo-client -- \
  --endpoint not-an-endpoint --message hello
```

Expected: non-zero exit with an endpoint validation error.

**Step 7: Verify workspace isolation**

Run:

```bash
cargo tree -p bcs --invert bcs-provider-demo-client
```

Expected: the BCS server does not depend on the demo client.

**Step 8: Commit**

```bash
git add src/bcs/Cargo.toml src/bcs/Cargo.lock \
  src/bcs/crates/tools/bcs-provider-demo-client
git commit -m "feat(bcs): add standalone provider gRPC demo client"
```

### Task 5: Verify cross-language interoperability and document usage

**Files:**
- Create: `src/bcs/sdks/README.md`
- Create: `src/bcs/scripts/test_provider_grpc_sdk_demo.sh`
- Modify: `src/bcs/api-contracts/README.md`

**Step 1: Write the smoke script before relying on it**

The script builds all three projects, starts the Python example on one
loopback port, calls it with the Rust executable, asserts the exact JSON, stops
it, repeats with the Java example on another port, and cleans up child
processes through a shell trap. It must not bind externally or delete files.

**Step 2: Run it and verify the first failure**

Run:

```bash
bash src/bcs/scripts/test_provider_grpc_sdk_demo.sh
```

Expected before completing orchestration: FAIL on the first missing readiness
or invocation behavior. Adjust the script, not the SDK contracts, until the
failure correctly represents cross-language reachability.

**Step 3: Complete minimal readiness orchestration**

Use bounded polling of the Rust CLI rather than fixed long sleeps. Capture
server logs in a temporary directory and show them on failure. Assert:

```json
{"message":"python: hello","implementation":"python"}
```

and:

```json
{"message":"java: hello","implementation":"java"}
```

**Step 4: Document the three manual commands**

Document Python server, Java server, and Rust client startup, the inheritance
extension points, exclusions, and the fact that this is not the final Provider
SSE or streaming contract. Add the Proto location to `api-contracts/README.md`.

**Step 5: Run focused verification**

```bash
uv run --with pytest pytest src/bcs/tests/provider_grpc_sdk_demo -q
uv run --project src/bcs/sdks/python --extra test pytest \
  src/bcs/sdks/python/tests -q
mvn -f src/bcs/sdks/java/pom.xml test
cargo test -p bcs-provider-demo-client
bash src/bcs/scripts/test_provider_grpc_sdk_demo.sh
cargo test -p bcs-protocol
git diff --check
```

Expected: every command passes and `git diff --check` has no output.

**Step 6: Commit**

```bash
git add src/bcs/sdks/README.md src/bcs/scripts/test_provider_grpc_sdk_demo.sh \
  src/bcs/api-contracts/README.md
git commit -m "test(bcs): verify provider gRPC SDK interoperability"
```

### Task 6: Final review and handoff

**Step 1: Inspect scope**

```bash
git status --short
git diff dev...HEAD --stat
git log --oneline dev..HEAD
```

Expected: only design/plan docs, the shared contract, two SDKs, the standalone
tool, focused docs/tests, and Cargo workspace metadata changed.

**Step 2: Run the verification-before-completion checklist**

Re-run every focused command from Task 5 and record exact pass counts. Do not
claim completion from earlier cached output.

**Step 3: Request code review**

Review architecture boundaries, generated-code handling, Java public-only
dependencies, error redaction, and actual Python/Java cross-language calls.
Address only concrete findings with new failing tests.

**Step 4: Prepare branch handoff**

Report the worktree path, branch, commits, commands run, results, remaining
production exclusions, and suggested next step for a versioned bidirectional
Provider protocol.
