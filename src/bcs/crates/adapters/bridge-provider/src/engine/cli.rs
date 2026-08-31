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
}
