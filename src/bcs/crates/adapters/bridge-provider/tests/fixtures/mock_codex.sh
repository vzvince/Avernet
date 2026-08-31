#!/usr/bin/env bash
# Mock cfuse --codex engine: take the trailing argv as the prompt and emit
# codex `exec --json` JSONL that echoes each non-blank prompt line as an
# `item.completed`/`agent_message` delta, then `turn.completed`. The CfuseCodex
# driver maps these to chat_delta StreamEvents so the chat.send SSE body carries
# the assembled prompt text (verifying inject prepending end-to-end).
#
# Per Task 13 amendment: emits JSONL (codex exec --json shape), NOT SSE.
while [ "$#" -gt 1 ]; do shift; done
prompt="$1"
printf '{"type":"thread.started","thread_id":"t-1"}\n'
printf '{"type":"turn.started"}\n'
while IFS= read -r line; do
    [ -z "$line" ] && continue
    printf '{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"%s"}}\n' "$line"
done <<< "$prompt"
printf '{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}\n'
