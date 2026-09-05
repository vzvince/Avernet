#!/usr/bin/env bash
# Mock cfuse --cc engine: read one stdin line (the user message JSON) then
# replay the recorded cc_turn.ndjson stream-json lines to stdout.
IFS= read -r _first
cat "$(dirname "$0")/cc_turn.ndjson"
