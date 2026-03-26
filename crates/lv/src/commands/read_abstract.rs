use anyhow::Result;
use clap::Parser;

use crate::client::LvClient;

#[derive(Parser)]
pub struct AbstractCmd {
    /// Viking URI to read abstract from
    uri: String,
}

impl AbstractCmd {
    pub async fn run(&self, client: &mut LvClient) -> Result<()> {
        let text = client.read_abstract(&self.uri).await?;
        println!("{text}");
        Ok(())
    }
}
