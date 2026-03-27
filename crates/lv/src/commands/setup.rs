use anyhow::Result;
use clap::Parser;
use std::process::Command;

#[derive(Parser)]
pub struct SetupCmd;

impl SetupCmd {
    pub async fn run(&self) -> Result<()> {
        println!("LiteVikings Setup");
        println!("=================\n");

        // 1. Check data directory
        let data_dir = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".litevikings")
            .join("data");
        if !data_dir.exists() {
            println!("[1/4] Creating data directory: {}", data_dir.display());
            std::fs::create_dir_all(&data_dir)?;
            println!("      Done.\n");
        } else {
            println!("[1/4] Data directory exists: {}\n", data_dir.display());
        }

        // 2. Check Ollama
        print!("[2/4] Checking Ollama... ");
        let ollama_ok = Command::new("ollama").arg("--version").output().is_ok();
        if ollama_ok {
            println!("found.");
        } else {
            println!("NOT FOUND.");
            println!();
            println!("      Please install Ollama:");
            println!("        macOS:  brew install ollama");
            println!("        Linux:  curl -fsSL https://ollama.com/install.sh | sh");
            println!();
            println!("      Then start it:");
            println!("        ollama serve");
            println!();
            println!("      Re-run `lv setup` after installing.");
            return Ok(());
        }

        // 3. Check/pull models
        println!();
        print!("[3/4] Checking chat model (qwen2.5:3b)... ");
        let chat_ok = check_ollama_model("qwen2.5:3b");
        if chat_ok {
            println!("ready.");
        } else {
            println!("not found. Pulling...");
            run_ollama_pull("qwen2.5:3b")?;
        }

        print!("      Checking embedding model (nomic-embed-text)... ");
        let embed_ok = check_ollama_model("nomic-embed-text");
        if embed_ok {
            println!("ready.");
        } else {
            println!("not found. Pulling...");
            run_ollama_pull("nomic-embed-text")?;
        }

        // 4. Test connectivity
        println!();
        print!("[4/4] Testing Ollama API... ");
        let test = reqwest::Client::new()
            .get("http://localhost:11434/v1/models")
            .send()
            .await;
        match test {
            Ok(resp) if resp.status().is_success() => {
                println!("OK.\n");
            }
            _ => {
                println!("FAILED.");
                println!("      Ollama is installed but not responding on localhost:11434.");
                println!("      Make sure `ollama serve` is running.\n");
                return Ok(());
            }
        }

        println!("Setup complete! Start the server with:\n");
        println!("  lv serve\n");
        println!("Then from another terminal:\n");
        println!("  lv status");
        println!("  lv add-resource ./README.md --to viking://resources/readme");
        println!("  lv find \"what is this project about\"");

        Ok(())
    }
}

fn check_ollama_model(model: &str) -> bool {
    Command::new("ollama")
        .args(["show", model])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_ollama_pull(model: &str) -> Result<()> {
    let status = Command::new("ollama").args(["pull", model]).status()?;
    if !status.success() {
        anyhow::bail!("Failed to pull model: {model}");
    }
    Ok(())
}
