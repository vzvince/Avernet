use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};

pub struct CliSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
}

impl CliSession {
    pub async fn spawn(
        bin: &Path,
        args: &[String],
        cwd: &Path,
        env: &[(String, String)],
    ) -> std::io::Result<Self> {
        let mut cmd = Command::new(bin);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| io_err("stdin not piped"))?;
        let stdout = child.stdout.take().ok_or_else(|| io_err("stdout not piped"))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| io_err("stderr not piped"))?;
        tokio::spawn(async move {
            let mut reader = BufReader::new(&mut stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => tracing::debug!(target: "bridge_provider::engine", stderr = line.trim_end()),
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    pub async fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await
    }

    pub async fn next_line(&mut self) -> std::io::Result<Option<String>> {
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line).await?;
        if n == 0 {
            return Ok(None);
        }
        Ok(Some(line.trim_end_matches(['\n', '\r']).to_string()))
    }

    /// 读取一个 SSE 块（`event:`/`data:` 行聚合到空行），返回 `(event, data)`。
    ///
    /// 多行 `data:` 按 SSE 规范用 `\n` 拼接；`event:` 取最后一次出现的值。
    /// EOF 且无残留（`saw_any == false`）→ `Ok(None)`；EOF 时仍有未分隔块 →
    /// 返回该部分块（`\n` 终结符丢失不丢已读内容）。空行作为块分隔符。
    pub async fn next_sse_block(&mut self) -> std::io::Result<Option<(String, String)>> {
        let mut event = String::new();
        let mut data_lines: Vec<String> = Vec::new();
        let mut saw_any = false;
        loop {
            match self.next_line().await? {
                None => return Ok(saw_any.then(|| (event, data_lines.join("\n")))),
                Some(line) if line.is_empty() => {
                    return Ok(if saw_any {
                        Some((event, data_lines.join("\n")))
                    } else {
                        None
                    });
                }
                Some(line) => {
                    saw_any = true;
                    if let Some(v) = line.strip_prefix("event: ") {
                        event = v.to_string();
                    }
                    if let Some(v) = line.strip_prefix("data: ") {
                        data_lines.push(v.to_string());
                    }
                }
            }
        }
    }

    pub async fn kill(&mut self) {
        if let Err(e) = self.child.start_kill() {
            tracing::debug!(target: "bridge_provider::engine", "kill failed: {e}");
        }
        let _ = self.child.wait().await;
    }
}

fn io_err(msg: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::BrokenPipe, msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cli_session_echo_and_kill() {
        let script = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mock_engine.sh");
        let mut cli = CliSession::spawn(
            Path::new("bash"),
            &[script.to_string()],
            Path::new("."),
            &[],
        )
        .await
        .unwrap();
        cli.write_line("hello").await.unwrap();
        let line = cli.next_line().await.unwrap().unwrap();
        assert_eq!(line, "ack:hello");
        cli.kill().await;
    }

    #[tokio::test]
    async fn cli_session_next_sse_block_parses_blocks_until_eof() {
        let script = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mock_sse.sh");
        let mut cli = CliSession::spawn(
            Path::new("bash"),
            &[script.to_string()],
            Path::new("."),
            &[],
        )
        .await
        .unwrap();
        cli.write_line("go").await.unwrap();

        let (event, data) = cli.next_sse_block().await.unwrap().unwrap();
        assert_eq!(event, "response.output_text.delta");
        assert_eq!(data, r#"{"type":"response.output_text.delta","delta":"hi"}"#);

        let (event, data) = cli.next_sse_block().await.unwrap().unwrap();
        assert_eq!(event, "response.completed");
        assert_eq!(data, r#"{"type":"response.completed"}"#);

        // EOF after the final blank line → None（无残留块）。
        assert!(cli.next_sse_block().await.unwrap().is_none());
        cli.kill().await;
    }
}
