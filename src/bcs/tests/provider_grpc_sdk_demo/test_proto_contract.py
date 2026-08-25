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
