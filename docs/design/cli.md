# lv (binary): CLI & TUI Design

## Overview

The `lv` crate is the binary entry point. It provides the CLI (via `clap`) and
optional TUI (via `ratatui`). **The CLI is a pure gRPC client** -- it never
instantiates the Engine directly. All operations go through the gRPC server.

For convenience, the CLI can auto-start a background server process if one
isn't running.

## Architecture

```
lv/
  src/
    main.rs             -- clap App, connect to server, dispatch
    commands/
      mod.rs
      add_resource.rs   -- lv add-resource
      ls.rs             -- lv ls
      tree.rs           -- lv tree
      read.rs           -- lv read
      write.rs          -- lv write
      find.rs           -- lv find
      search.rs         -- lv search
      grep.rs           -- lv grep
      glob.rs           -- lv glob
      session.rs        -- lv session {create,list,show,delete,message,commit}
      admin.rs          -- lv admin {init,status}
      serve.rs          -- lv serve (starts lv-server in foreground)
      import_legacy.rs  -- lv import-legacy (TODO)
    client.rs           -- gRPC client wrapper (typed convenience methods)
    autostart.rs        -- Auto-start background server daemon
    output.rs           -- JSON/Table/Plain output formatting
    config.rs           -- Config loading (~/.litevikings/config.toml)
    tui/
      mod.rs
      app.rs            -- TUI application state
      browser.rs        -- File browser widget
      search.rs         -- Search widget
      render.rs         -- Rendering logic
```

## Command Structure

```
lv [global-options] <command> [args]

Global options:
  --server <ADDR>    gRPC server address (default: 127.0.0.1:50051)
  --api-key <KEY>    API key for authentication
  --auto-start       Start server in background if not running (default: true)
  --data-dir <PATH>  Data directory for auto-started server (default: ~/.litevikings)
  --config <PATH>    Config file path
  --format <FORMAT>  Output format: json | table | plain (default: plain)
  -v, --verbose      Increase log verbosity

Commands:
  add-resource       Import a file, URL, or directory
  ls                 List directory contents
  tree               Show directory tree
  read               Read content (L2)
  abstract           Read L0 abstract
  overview           Read L1 overview
  write              Write content to a URI
  find               Semantic search
  search             Search with session context
  grep               Text pattern search
  glob               URI pattern matching
  session            Session management
    create           Create a new session
    list             List sessions
    show             Show session details
    delete           Delete a session
    message          Add a message to a session
    commit           Trigger compression + memory extraction
  admin              Administrative operations
    init             Initialize preset directories
    status           System status
  serve              Start server in foreground (both gRPC + HTTP)
  tui                Launch interactive TUI
  import-legacy      Import from AGFS format (TODO)
```

## gRPC Client Wrapper

Typed convenience layer over the generated tonic client stubs.

```rust
use lv_proto::litevikings::v1::*;
use lv_proto::litevikings::v1::filesystem_service_client::FilesystemServiceClient;
// ... other clients ...

pub struct LvClient {
    fs: FilesystemServiceClient<Channel>,
    resources: ResourcesServiceClient<Channel>,
    sessions: SessionsServiceClient<Channel>,
    search: SearchServiceClient<Channel>,
    relations: RelationsServiceClient<Channel>,
    admin: AdminServiceClient<Channel>,
    api_key: Option<String>,
}

impl LvClient {
    pub async fn connect(addr: &str, api_key: Option<String>) -> Result<Self> {
        let channel = Channel::from_shared(format!("http://{}", addr))?
            .connect()
            .await?;
        Ok(Self {
            fs: FilesystemServiceClient::new(channel.clone()),
            resources: ResourcesServiceClient::new(channel.clone()),
            sessions: SessionsServiceClient::new(channel.clone()),
            search: SearchServiceClient::new(channel.clone()),
            relations: RelationsServiceClient::new(channel.clone()),
            admin: AdminServiceClient::new(channel),
            api_key,
        })
    }

    /// Inject auth metadata into every request.
    fn request<T>(&self, inner: T) -> tonic::Request<T> {
        let mut req = tonic::Request::new(inner);
        if let Some(key) = &self.api_key {
            req.metadata_mut().insert("x-api-key", key.parse().unwrap());
        }
        req
    }

    // --- Filesystem ---

    pub async fn ls(&mut self, uri: &str, opts: &LsOptions) -> Result<Vec<DirEntry>> {
        let resp = self.fs.ls(self.request(LsRequest {
            uri: uri.to_string(),
            simple: opts.simple,
            recursive: opts.recursive,
            output: opts.output.clone(),
            abs_limit: opts.abs_limit,
            show_all_hidden: opts.show_all_hidden,
            node_limit: opts.node_limit,
        })).await?;
        Ok(resp.into_inner().entries)
    }

    pub async fn read(&mut self, uri: &str) -> Result<String> {
        let resp = self.fs.read(self.request(ReadRequest {
            uri: uri.to_string(),
        })).await?;
        Ok(resp.into_inner().content)
    }

    // --- Search ---

    pub async fn find(&mut self, query: &str, target_uri: Option<&str>, limit: i32) -> Result<FindResponse> {
        let resp = self.search.find(self.request(FindRequest {
            query: query.to_string(),
            target_uri: target_uri.map(String::from),
            limit,
            ..Default::default()
        })).await?;
        Ok(resp.into_inner())
    }

    // --- Resources ---

    pub async fn add_resource(&mut self, req: AddResourceRequest) -> Result<AddResourceResponse> {
        let resp = self.resources.add_resource(self.request(req)).await?;
        Ok(resp.into_inner())
    }

    pub async fn wait_processed(&mut self) -> Result<()> {
        self.resources.wait_processed(self.request(WaitProcessedRequest {
            timeout: None,
        })).await?;
        Ok(())
    }

    // --- Sessions ---

    pub async fn create_session(&mut self) -> Result<CreateSessionResponse> {
        let resp = self.sessions.create(self.request(CreateSessionRequest {
            session_id: None,
        })).await?;
        Ok(resp.into_inner())
    }

    // ... etc for all operations ...
}
```

## Auto-Start

When the CLI can't connect to the configured gRPC address:

```rust
pub async fn ensure_server_running(config: &CliConfig) -> Result<String> {
    let addr = &config.server_addr;

    // 1. Try to connect
    if try_connect(addr).await.is_ok() {
        return Ok(addr.clone());
    }

    if !config.auto_start {
        anyhow::bail!(
            "Cannot connect to server at {}. Start it with `lv serve` or use --auto-start.",
            addr
        );
    }

    // 2. Spawn server as background daemon
    tracing::info!("Auto-starting server...");
    let child = std::process::Command::new(std::env::current_exe()?)
        .args(["serve", "--data-dir", &config.data_dir.to_string_lossy()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    tracing::info!("Server started (pid: {})", child.id());

    // 3. Wait for gRPC port to become available (max 10s)
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if try_connect(addr).await.is_ok() {
            return Ok(addr.clone());
        }
    }

    anyhow::bail!("Server failed to start within 10 seconds");
}

async fn try_connect(addr: &str) -> Result<()> {
    let channel = Channel::from_shared(format!("http://{}", addr))?
        .connect_timeout(std::time::Duration::from_millis(200))
        .connect()
        .await?;
    // Quick health check: call admin.status
    let mut client = AdminServiceClient::new(channel);
    client.status(tonic::Request::new(StatusRequest {})).await?;
    Ok(())
}
```

## Main Entry Point

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::init();

    // `lv serve` runs the server in foreground -- doesn't need a client
    if let Command::Serve(cmd) = &cli.command {
        return cmd.run().await;
    }

    // All other commands need a gRPC connection
    let config = load_config(&cli)?;
    let addr = ensure_server_running(&config).await?;
    let mut client = LvClient::connect(&addr, config.api_key.clone()).await?;

    match cli.command {
        Command::Ls(cmd) => cmd.run(&mut client, &cli.format).await,
        Command::Find(cmd) => cmd.run(&mut client, &cli.format).await,
        Command::AddResource(cmd) => cmd.run(&mut client, &cli.format).await,
        Command::Session(cmd) => cmd.run(&mut client, &cli.format).await,
        Command::Admin(cmd) => cmd.run(&mut client, &cli.format).await,
        Command::Tui(_) => tui::run(&mut client).await,
        Command::Serve(_) => unreachable!(),
        // ...
    }
}
```

## Command Example: `lv find`

```rust
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
        let resp = client.find(&self.query, self.target.as_deref(), self.limit).await?;

        match format {
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&resp)?);
            }
            OutputFormat::Plain => {
                for ctx in &resp.resources {
                    println!("{} (score: {:.3})", ctx.uri, ctx.score);
                    if !ctx.abstract_text.is_empty() {
                        println!("  {}", ctx.abstract_text);
                    }
                }
                println!("\n{} results, {} searched, {} rounds",
                    resp.resources.len(), resp.total_searched, resp.rounds);
            }
            OutputFormat::Table => { /* tabulate */ }
        }
        Ok(())
    }
}
```

## Configuration

`~/.litevikings/config.toml`:

```toml
# gRPC server address (for CLI to connect to)
server_addr = "127.0.0.1:50051"
api_key = ""
auto_start = true
data_dir = "~/.litevikings"

# LLM settings (used by auto-started server)
[llm.embedder]
base_url = "https://api.openai.com/v1"
api_key = "sk-..."
model = "text-embedding-3-small"
dimension = 1536
batch_size = 100

[llm.chat]
base_url = "https://api.openai.com/v1"
api_key = "sk-..."
model = "gpt-4o-mini"
temperature = 0.3

# Server settings (for `lv serve`)
[server]
grpc_addr = "0.0.0.0:50051"
http_addr = "0.0.0.0:1933"
auth_mode = "trusted"
```

## Crate Dependencies

```toml
[dependencies]
lv-core = { path = "../lv-core" }
lv-proto = { path = "../lv-proto" }
lv-server = { path = "../lv-server" }  # only for `lv serve` command

tokio = { workspace = true }
tonic = { workspace = true }
prost = { workspace = true }
clap = { workspace = true }
ratatui = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
toml = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
anyhow = { workspace = true }
```

## TODO / Deferred

- **Shell completions** (clap_complete for bash/zsh/fish)
- **TUI search results panel**
- **Pipe support** (`cat file | lv write viking://...`)
- **Server management commands** (`lv server stop`, `lv server restart`)
- **Connection pooling / retry** for flaky network to remote servers
