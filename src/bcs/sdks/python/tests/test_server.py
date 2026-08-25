import re
from pathlib import Path

import grpc
import pytest

from bcs_provider_sdk import ProviderServer, ProviderService
from bcs_provider_sdk._generated import provider_demo_pb2, provider_demo_pb2_grpc


SDK_ROOT = Path(__file__).parents[1]


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


def test_runtime_dependency_floors_match_generated_modules() -> None:
    grpc_source = (
        SDK_ROOT
        / "src/bcs_provider_sdk/_generated/provider_demo_pb2_grpc.py"
    ).read_text(encoding="utf-8")
    protobuf_source = (
        SDK_ROOT
        / "src/bcs_provider_sdk/_generated/provider_demo_pb2.py"
    ).read_text(encoding="utf-8")
    grpc_version = re.search(
        r"GRPC_GENERATED_VERSION = '([^']+)'",
        grpc_source,
    )
    protobuf_version = re.search(
        r"# Protobuf Python Version: ([0-9.]+)",
        protobuf_source,
    )
    assert grpc_version is not None
    assert protobuf_version is not None

    project = (SDK_ROOT / "pyproject.toml").read_text(encoding="utf-8")

    assert f'"grpcio>={grpc_version.group(1)},<2"' in project
    assert f'"grpcio-tools>={grpc_version.group(1)},<2"' in project
    assert f'"protobuf>={protobuf_version.group(1)},<7"' in project


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
