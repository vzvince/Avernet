pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
pub const HEARTBEAT: &str = ": heartbeat\n\n";

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("SSE frame too large: {0} bytes")]
    FrameTooLarge(usize),
    #[error("serialize SSE data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("SSE data must be single-line JSON")]
    MultilineData,
}

/// Encode one SSE frame from a pre-serialized single-line JSON string.
///
/// `data_json` must already be compact, single-line JSON (no `\n`/`\r`).
/// Callers holding a `serde_json::Value` should serialize it first with
/// `serde_json::to_string`; its error converts into `FrameError::Json` via `?`.
pub fn encode_frame(event: &str, id: Option<u64>, data_json: &str) -> Result<String, FrameError> {
    // SSE data: 行必须单行；先拒绝内嵌换行，避免拆成多帧
    if data_json.contains('\n') || data_json.contains('\r') {
        return Err(FrameError::MultilineData);
    }
    let mut frame = String::with_capacity(event.len() + data_json.len() + 24);
    frame.push_str("event: ");
    frame.push_str(event);
    frame.push('\n');
    if let Some(id) = id {
        frame.push_str("id: ");
        frame.push_str(&id.to_string());
        frame.push('\n');
    }
    frame.push_str("data: ");
    frame.push_str(data_json);
    frame.push_str("\n\n");
    if frame.len() > MAX_FRAME_BYTES {
        return Err(FrameError::FrameTooLarge(frame.len()));
    }
    Ok(frame)
}
