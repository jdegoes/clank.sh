#[tokio::main]
async fn main() {
    let shell = clank::build_shell().await;
    clank::run_repl(shell).await;
}
