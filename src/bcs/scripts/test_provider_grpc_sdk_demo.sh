#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BCS_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PYTHON_PROJECT="$BCS_ROOT/sdks/python"
JAVA_POM="$BCS_ROOT/sdks/java/pom.xml"
CLIENT="$BCS_ROOT/target/debug/bcs-provider-demo-client"
LOG_DIR="$(mktemp -d -t bcs-provider-grpc-sdk-demo.XXXXXX)"
SERVER_PID=""

cleanup() {
    if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}

trap cleanup EXIT INT TERM

free_port() {
    python3 -c 'import socket; sock = socket.socket(); sock.bind(("127.0.0.1", 0)); print(sock.getsockname()[1]); sock.close()'
}

wait_for_response() {
    local name="$1"
    local endpoint="$2"
    local expected="$3"
    local server_log="$4"
    local client_log="$LOG_DIR/${name}-client.log"
    local actual

    for _attempt in {1..100}; do
        if ! kill -0 "$SERVER_PID" 2>/dev/null; then
            echo "$name Provider server exited before becoming ready." >&2
            cat "$server_log" >&2
            return 1
        fi
        if actual="$($CLIENT --endpoint "$endpoint" --message hello 2>"$client_log")"; then
            if [[ "$actual" != "$expected" ]]; then
                echo "unexpected response: $actual" >&2
                echo "expected response:   $expected" >&2
                cat "$server_log" >&2
                return 1
            fi
            return 0
        fi
        sleep 0.1
    done

    echo "Timed out waiting for $name Provider server at $endpoint." >&2
    cat "$server_log" >&2
    cat "$client_log" >&2
    return 1
}

stop_server() {
    if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    SERVER_PID=""
}

echo "Building the standalone Rust client..."
cargo build --manifest-path "$BCS_ROOT/Cargo.toml" --package bcs-provider-demo-client

python_port="$(free_port)"
python_log="$LOG_DIR/python.log"
echo "Starting the Python SDK example on 127.0.0.1:$python_port..."
uv run --project "$PYTHON_PROJECT" --extra test \
    python "$PYTHON_PROJECT/examples/echo_server.py" --port "$python_port" \
    >"$python_log" 2>&1 &
SERVER_PID=$!
wait_for_response \
    "Python" \
    "http://127.0.0.1:$python_port" \
    '{"message":"python: hello","implementation":"python"}' \
    "$python_log"
stop_server

java_port="$(free_port)"
java_log="$LOG_DIR/java.log"
echo "Starting the Java SDK example on 127.0.0.1:$java_port..."
mvn -f "$JAVA_POM" compile exec:java \
    -Dexec.mainClass=com.avernet.bcs.provider.sdk.example.EchoServer \
    -Dexec.args="--port $java_port" \
    >"$java_log" 2>&1 &
SERVER_PID=$!
wait_for_response \
    "Java" \
    "http://127.0.0.1:$java_port" \
    '{"message":"java: hello","implementation":"java"}' \
    "$java_log"
stop_server

echo "Provider gRPC SDK interoperability smoke test passed."
echo "Logs: $LOG_DIR"
