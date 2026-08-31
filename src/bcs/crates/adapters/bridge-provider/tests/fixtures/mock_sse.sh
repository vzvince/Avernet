#!/usr/bin/env bash
# mock cfuse codex：读一行 stdin（启动握手），向 stdout 吐两个 SSE 块（delta + completed），然后退出。
# 块以空行分隔，与 codex_turn.sse 同形；用于驱动 CliSession::next_sse_block。
IFS= read -r _line
printf 'event: response.output_text.delta\ndata: {"type":"response.output_text.delta","delta":"hi"}\n\n'
printf 'event: response.completed\ndata: {"type":"response.completed"}\n\n'
