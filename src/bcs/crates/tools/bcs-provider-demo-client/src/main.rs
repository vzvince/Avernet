use bcs_provider_demo_client::{Cli, execute};
use clap::Parser as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let output = execute(Cli::parse()).await?;
    println!("{output}");
    Ok(())
}
