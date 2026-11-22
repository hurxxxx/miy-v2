mod api;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if api::handle_cli(std::env::args().nth(1).as_deref())? {
        return Ok(());
    }
    api::serve().await
}
