use anyhow::Result;
use clap::Parser;

use crate::client::LvClient;

#[derive(Parser)]
pub struct RmCmd {
    /// Viking URI to remove
    uri: String,

    /// Remove recursively
    #[arg(short, long)]
    recursive: bool,
}

impl RmCmd {
    pub async fn run(&self, client: &mut LvClient) -> Result<()> {
        client.rm(&self.uri, self.recursive).await?;
        println!("removed {}", self.uri);
        Ok(())
    }
}
