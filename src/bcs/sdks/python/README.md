# Python Provider SDK Demo

This package demonstrates a minimal inheritable gRPC Provider server. It is a
transport proof of concept, not the final BCS Provider streaming SDK.

## Extend the SDK

```python
from bcs_provider_sdk import ProviderService


class MyProvider(ProviderService):
    @property
    def implementation(self) -> str:
        return "my-python-provider"

    async def invoke(self, message: str) -> str:
        return f"received: {message}"
```

Pass the subclass instance to `ProviderServer` to host it with a standard
`grpc.aio` server.

## Run the example

From the Avernet repository root:

```bash
uv run --project src/bcs/sdks/python --extra test \
  python src/bcs/sdks/python/examples/echo_server.py --port 50051
```

Regenerate the checked-in Protobuf modules after changing the canonical
contract:

```bash
uv run --project src/bcs/sdks/python --extra test \
  bash src/bcs/sdks/python/scripts/generate_proto.sh
```
