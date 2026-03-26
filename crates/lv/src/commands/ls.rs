use anyhow::Result;
use clap::Parser;

use crate::client::LvClient;
use crate::OutputFormat;

#[derive(Parser)]
pub struct LsCmd {
    /// Viking URI to list
    #[arg(default_value = "viking://")]
    uri: String,

    /// List recursively
    #[arg(short, long)]
    recursive: bool,

    /// Max entries
    #[arg(long, default_value = "100")]
    limit: i32,
}

impl LsCmd {
    pub async fn run(&self, client: &mut LvClient, format: &OutputFormat) -> Result<()> {
        let entries = client.ls(&self.uri, self.recursive, self.limit).await?;

        match format {
            OutputFormat::Json => {
                let json: Vec<serde_json::Value> = entries
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "uri": e.uri,
                            "is_leaf": e.is_leaf,
                            "abstract": e.abstract_text,
                            "type": e.context_type,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&json)?);
            }
            OutputFormat::Plain => {
                if entries.is_empty() {
                    println!("(empty)");
                } else {
                    for e in &entries {
                        let icon = if e.is_leaf { " " } else { "/" };
                        let abs = if e.abstract_text.is_empty() {
                            String::new()
                        } else {
                            format!("  -- {}", e.abstract_text)
                        };
                        println!("{}{icon}{abs}", e.uri);
                    }
                }
            }
        }
        Ok(())
    }
}
