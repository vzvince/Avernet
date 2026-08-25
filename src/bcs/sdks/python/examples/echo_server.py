import argparse
import asyncio

from bcs_provider_sdk import ProviderServer, ProviderService


class EchoProvider(ProviderService):
    @property
    def implementation(self) -> str:
        return "python"

    async def invoke(self, message: str) -> str:
        return f"python: {message}"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run the Python Provider SDK demo")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", default=50051, type=int)
    return parser.parse_args()


async def run(host: str, port: int) -> None:
    server = ProviderServer(EchoProvider(), host=host, port=port)
    await server.start()
    print(f"Python Provider demo listening on {host}:{server.bound_port}", flush=True)
    try:
        await server.wait_for_termination()
    finally:
        await server.stop()


if __name__ == "__main__":
    args = parse_args()
    try:
        asyncio.run(run(args.host, args.port))
    except KeyboardInterrupt:
        pass
