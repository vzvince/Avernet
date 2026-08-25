import grpc
import pytest

from bcs_provider_sdk import ProviderServer, ProviderService
from bcs_provider_sdk._generated import provider_demo_pb2, provider_demo_pb2_grpc


class EchoProvider(ProviderService):
    @property
    def implementation(self) -> str:
        return "python"

    async def invoke(self, message: str) -> str:
        return f"python: {message}"


class FailingProvider(ProviderService):
    @property
    def implementation(self) -> str:
        return "python"

    async def invoke(self, message: str) -> str:
        del message
        raise RuntimeError("secret detail")


@pytest.mark.asyncio
async def test_subclass_receives_grpc_invocation() -> None:
    server = ProviderServer(EchoProvider(), host="127.0.0.1", port=0)
    await server.start()
    channel = grpc.aio.insecure_channel(f"127.0.0.1:{server.bound_port}")

    try:
        stub = provider_demo_pb2_grpc.ProviderDemoStub(channel)
        response = await stub.Invoke(provider_demo_pb2.InvokeRequest(message="hello"))
    finally:
        await channel.close()
        await server.stop()

    assert server.bound_port > 0
    assert response.message == "python: hello"
    assert response.implementation == "python"


@pytest.mark.asyncio
async def test_handler_error_is_redacted_as_internal() -> None:
    server = ProviderServer(FailingProvider(), host="127.0.0.1", port=0)
    await server.start()
    channel = grpc.aio.insecure_channel(f"127.0.0.1:{server.bound_port}")

    try:
        stub = provider_demo_pb2_grpc.ProviderDemoStub(channel)
        with pytest.raises(grpc.aio.AioRpcError) as caught:
            await stub.Invoke(provider_demo_pb2.InvokeRequest(message="hello"))
    finally:
        await channel.close()
        await server.stop()

    assert caught.value.code() is grpc.StatusCode.INTERNAL
    assert caught.value.details() == "provider invocation failed"
    assert "secret detail" not in caught.value.details()
