from __future__ import annotations

import logging

import grpc

from bcs_provider_sdk._generated import provider_demo_pb2, provider_demo_pb2_grpc
from bcs_provider_sdk.service import ProviderService


logger = logging.getLogger(__name__)


class _ProviderDemoServicer(provider_demo_pb2_grpc.ProviderDemoServicer):
    def __init__(self, service: ProviderService) -> None:
        self._service = service

    async def Invoke(
        self,
        request: provider_demo_pb2.InvokeRequest,
        context: grpc.aio.ServicerContext,
    ) -> provider_demo_pb2.InvokeResponse:
        try:
            message = await self._service.invoke(request.message)
        except Exception:  # noqa: BLE001 - SDK boundary maps handler failures.
            logger.exception("Provider invocation failed")
            await context.abort(
                grpc.StatusCode.INTERNAL,
                "provider invocation failed",
            )
        return provider_demo_pb2.InvokeResponse(
            message=message,
            implementation=self._service.implementation,
        )


class ProviderServer:
    """Owns the lifecycle of a standard grpc.aio Provider demo server."""

    def __init__(
        self,
        service: ProviderService,
        *,
        host: str = "127.0.0.1",
        port: int = 50051,
    ) -> None:
        self._host = host
        self._port = port
        self._server = grpc.aio.server()
        self._bound_port: int | None = None
        provider_demo_pb2_grpc.add_ProviderDemoServicer_to_server(
            _ProviderDemoServicer(service),
            self._server,
        )

    @property
    def bound_port(self) -> int:
        if self._bound_port is None:
            raise RuntimeError("provider server has not started")
        return self._bound_port

    async def start(self) -> None:
        if self._bound_port is not None:
            return
        bound_port = self._server.add_insecure_port(f"{self._host}:{self._port}")
        if bound_port == 0:
            raise RuntimeError(f"failed to bind provider server on {self._host}:{self._port}")
        await self._server.start()
        self._bound_port = bound_port

    async def wait_for_termination(self) -> None:
        await self._server.wait_for_termination()

    async def stop(self, grace: float = 0) -> None:
        await self._server.stop(grace)
