use anyhow::Result;
use clap::Parser;

use crate::client::LvClient;

#[derive(Parser)]
pub struct WriteCmd {
    /// Viking URI to write to
    uri: String,

    /// Content to write
    content: String,
}

impl WriteCmd {
    pub async fn run(&self, client: &mut LvClient) -> Result<()> {
        client.write(&self.uri, &self.content).await?;
        println!("wrote to {}", self.uri);
        Ok(())
    }
}
