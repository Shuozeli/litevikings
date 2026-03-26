use anyhow::Result;
use clap::Parser;

use crate::client::LvClient;

#[derive(Parser)]
pub struct ReadCmd {
    /// Viking URI to read
    uri: String,
}

impl ReadCmd {
    pub async fn run(&self, client: &mut LvClient) -> Result<()> {
        let content = client.read(&self.uri).await?;
        print!("{content}");
        Ok(())
    }
}
