use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::client::LvClient;

#[derive(Parser)]
pub struct SessionCmd {
    #[command(subcommand)]
    command: SessionSubcommand,
}

#[derive(Subcommand)]
enum SessionSubcommand {
    /// Create a new session
    Create,
    /// List sessions
    List,
    /// Show session details
    Show { session_id: String },
    /// Delete a session
    Delete { session_id: String },
    /// Add a message to a session
    Message {
        session_id: String,
        /// Role: user, assistant, or system
        #[arg(long, default_value = "user")]
        role: String,
        /// Message text
        text: String,
    },
    /// Get messages from a session
    Messages { session_id: String },
    /// Commit: compress + extract memories
    Commit { session_id: String },
}

impl SessionCmd {
    pub async fn run(&self, client: &mut LvClient) -> Result<()> {
        match &self.command {
            SessionSubcommand::Create => {
                let resp = client.session_create().await?;
                println!("session_id: {}", resp.session_id);
                println!("session_uri: {}", resp.session_uri);
            }
            SessionSubcommand::List => {
                let sessions = client.session_list().await?;
                if sessions.is_empty() {
                    println!("(no sessions)");
                } else {
                    for s in &sessions {
                        println!("{} ({})", s.session_id, s.session_uri);
                    }
                }
            }
            SessionSubcommand::Show { session_id } => {
                let resp = client.session_get(session_id).await?;
                println!("session_id: {}", resp.session_id);
                println!("session_uri: {}", resp.session_uri);
                println!("owner_user: {}", resp.owner_user);
            }
            SessionSubcommand::Delete { session_id } => {
                client.session_delete(session_id).await?;
                println!("deleted {session_id}");
            }
            SessionSubcommand::Message {
                session_id,
                role,
                text,
            } => {
                client.session_add_message(session_id, role, text).await?;
                println!("message added to {session_id}");
            }
            SessionSubcommand::Messages { session_id } => {
                let msgs = client.session_get_messages(session_id).await?;
                if msgs.is_empty() {
                    println!("(no messages)");
                } else {
                    for m in &msgs {
                        println!("[{}] {}", m.role, m.timestamp);
                    }
                }
            }
            SessionSubcommand::Commit { session_id } => {
                let resp = client.session_commit(session_id).await?;
                println!(
                    "committed session {session_id}: {} memories extracted",
                    resp.memories_extracted
                );
            }
        }
        Ok(())
    }
}
