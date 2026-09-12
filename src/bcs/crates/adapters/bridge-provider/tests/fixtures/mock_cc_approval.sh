#!/usr/bin/env bash
# Mock cfuse --cc engine for the HITL interaction roundtrip (Task 12):
# read the user message, emit a can_use_tool control_request for Bash, wait for
# the engine's control_response on stdin, then emit a terminal result keyed on
# the received behavior.
#
# Uses /bin/echo (external, flushes its stdio buffer on exit) for the
# control_request line so the driver can observe it before this script blocks
# on `read` — a bash `printf` builtin would block-buffer a non-tty stdout and
# deadlock the turn (driver waits for the control_request while the mock waits
# for the control_response).
IFS= read -r _user
/bin/echo '{"type":"control_request","request":{"subtype":"can_use_tool","request_id":"req-1","tool_name":"Bash","input":{"command":"npm run deploy"}}}'
IFS= read -r ctrl
behavior=$(printf '%s\n' "$ctrl" | sed -n 's/.*"behavior":"\([^"]*\)".*/\1/p')
if [ "$behavior" = "allow" ]; then
  /bin/echo '{"type":"result","subtype":"success","result":"approved","session_id":"cc-approval-1"}'
else
  /bin/echo '{"type":"result","subtype":"success","result":"denied","session_id":"cc-approval-1"}'
fi
