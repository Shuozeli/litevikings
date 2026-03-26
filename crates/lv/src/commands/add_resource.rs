use anyhow::Result;
use clap::Parser;

use crate::client::LvClient;

#[derive(Parser)]
pub struct AddResourceCmd {
    /// Source: URL (http/https) or local file path
    source: String,

    /// Target Viking URI (auto-generated if not specified)
    #[arg(long)]
    to: Option<String>,

    /// Wait for L0/L1 generation + embedding to complete
    #[arg(long)]
    wait: bool,
}

impl AddResourceCmd {
    pub async fn run(&self, client: &mut LvClient) -> Result<()> {
        let result = client
            .add_resource(&self.source, self.to.as_deref(), self.wait)
            .await?;

        println!("root_uri: {}", result.root_uri);
        println!("nodes_created: {}", result.nodes_created);
        println!("processing_queued: {}", result.processing_queued);

        if self.wait {
            println!("processing complete.");
        }

        Ok(())
    }
}
