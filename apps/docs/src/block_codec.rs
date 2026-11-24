//! A single headless BlockNote instance shares the browser schema. Restart the
//! child on timeout or broken pipes; a failed conversion never changes app data.
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{path::Path, process::Stdio, sync::LazyLock, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};

struct Codec {
    _child: Child,
    input: ChildStdin,
    output: Lines<BufReader<ChildStdout>>,
}
impl Codec {
    fn start() -> Result<Self> {
        let installed = Path::new("/miy/server/codec.mjs");
        let path = if installed.is_file() {
            installed
        } else {
            Path::new("dist/server/codec.mjs")
        };
        let mut child = Command::new("node")
            .arg("--max-old-space-size=192")
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let input = child.stdin.take().context("codec stdin")?;
        let output = BufReader::new(child.stdout.take().context("codec stdout")?).lines();
        Ok(Self {
            _child: child,
            input,
            output,
        })
    }
    async fn request(&mut self, request: Value) -> Result<Value> {
        let mut line = serde_json::to_vec(&request)?;
        line.push(b'\n');
        self.input.write_all(&line).await?;
        let reply = self
            .output
            .next_line()
            .await?
            .context("document codec closed")?;
        let result: Value = serde_json::from_str(&reply)?;
        ensure!(result.get("error").is_none(), "invalid BlockNote document");
        Ok(result)
    }
}
async fn call(request: Value) -> Result<Value> {
    static CODEC: LazyLock<Mutex<Option<Codec>>> = LazyLock::new(|| Mutex::new(None));
    let mut guard = CODEC.lock().await;
    let mut codec = guard.take().map(Ok).unwrap_or_else(Codec::start)?;
    let result = tokio::time::timeout(Duration::from_secs(10), codec.request(request)).await??;
    *guard = Some(codec);
    Ok(result)
}
pub async fn seed(blocks: &Value) -> Result<Vec<u8>> {
    let result = call(json!({"operation":"seed","blocks":blocks})).await?;
    Ok(STANDARD.decode(result["state"].as_str().context("missing codec state")?)?)
}
pub async fn project(state: &[u8]) -> Result<Value> {
    let mut result = call(json!({"operation":"project","state":STANDARD.encode(state)})).await?;
    ensure!(result["blocks"].is_array(), "invalid codec projection");
    Ok(result["blocks"].take())
}
