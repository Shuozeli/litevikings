use anyhow::Result;
use clap::Parser;

use crate::client::LvClient;

#[derive(Parser)]
pub struct MkdirCmd {
    /// Viking URI to create
    uri: String,
}

impl MkdirCmd {
    pub async fn run(&self, client: &mut LvClient) -> Result<()> {
        client.mkdir(&self.uri).await?;
        println!("created {}", self.uri);
        Ok(())
    }
}
