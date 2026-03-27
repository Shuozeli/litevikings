use anyhow::Result;
use clap::Parser;
use std::io::Read;

use crate::client::LvClient;

#[derive(Parser)]
pub struct WriteCmd {
    /// Viking URI to write to
    uri: String,

    /// Content to write (omit to read from stdin)
    content: Option<String>,

    /// Read content from stdin
    #[arg(long)]
    stdin: bool,
}

impl WriteCmd {
    pub async fn run(&self, client: &mut LvClient) -> Result<()> {
        let content = if self.stdin || self.content.is_none() {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf
        } else {
            self.content.clone().unwrap()
        };

        client.write(&self.uri, &content).await?;
        println!("wrote to {}", self.uri);
        Ok(())
    }
}
