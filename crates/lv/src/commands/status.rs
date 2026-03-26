use anyhow::Result;
use clap::Parser;

use crate::client::LvClient;

#[derive(Parser)]
pub struct StatusCmd;

impl StatusCmd {
    pub async fn run(&self, client: &mut LvClient) -> Result<()> {
        let status = client.status().await?;
        println!("contexts:  {}", status.context_count);
        println!("sessions:  {}", status.session_count);
        println!("vectors:   {}", status.vector_count);
        println!("db size:   {} bytes", status.db_size_bytes);
        Ok(())
    }
}
