#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
sdk_dir="$(cd "${script_dir}/.." && pwd)"
bcs_dir="$(cd "${sdk_dir}/../.." && pwd)"
proto_source="${bcs_dir}/api-contracts/provider-demo/v1/provider_demo.proto"
generation_root="$(mktemp -d)"

cleanup() {
    rm -rf "${generation_root}"
}
trap cleanup EXIT

proto_package_dir="${generation_root}/bcs_provider_sdk/_generated"
mkdir -p "${proto_package_dir}"
cp "${proto_source}" "${proto_package_dir}/provider_demo.proto"

python -m grpc_tools.protoc \
    --proto_path="${generation_root}" \
    --python_out="${sdk_dir}/src" \
    --grpc_python_out="${sdk_dir}/src" \
    "${proto_package_dir}/provider_demo.proto"

