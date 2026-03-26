use anyhow::Result;
use clap::Parser;

use crate::client::LvClient;
use crate::OutputFormat;

#[derive(Parser)]
pub struct FindCmd {
    /// Search query
    query: String,

    /// Target URI scope
    #[arg(long)]
    target: Option<String>,

    /// Max results
    #[arg(long, default_value = "10")]
    limit: i32,
}

impl FindCmd {
    pub async fn run(&self, client: &mut LvClient, format: &OutputFormat) -> Result<()> {
        let resp = client
            .find(&self.query, self.target.as_deref(), self.limit)
            .await?;

        match format {
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "query": resp.query,
                        "resources": resp.resources.iter().map(|r| serde_json::json!({
                            "uri": r.uri,
                            "score": r.score,
                            "abstract": r.abstract_text,
                        })).collect::<Vec<_>>(),
                        "total_searched": resp.total_searched,
                        "rounds": resp.rounds,
                    }))?
                );
            }
            OutputFormat::Plain => {
                if resp.resources.is_empty() {
                    println!("no results");
                } else {
                    for r in &resp.resources {
                        println!("{} (score: {:.3})", r.uri, r.score);
                        if !r.abstract_text.is_empty() {
                            println!("  {}", r.abstract_text);
                        }
                    }
                    println!(
                        "\n{} results, {} searched, {} rounds",
                        resp.resources.len(),
                        resp.total_searched,
                        resp.rounds
                    );
                }
            }
        }
        Ok(())
    }
}
