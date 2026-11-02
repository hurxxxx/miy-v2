mod todos;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = miy_app_sdk::database().await?;
    let app = todos::router().with_state(pool);
    miy_app_sdk::serve(app).await
}
