use std::sync::Arc;

use lv_core::uri::VikingUri;
use lv_core::CoreError;

use super::markdown::parse_markdown;
use crate::service::context::RequestContext;
use crate::storage::{Database, EmbeddingQueue, EmbeddingTask, VikingFs};

#[derive(Debug)]
pub struct ImportRequest {
    /// Source: URL (http/https) or local file path
    pub source: String,
    /// Target URI (e.g., "viking://resources/my-doc")
    pub target_uri: Option<String>,
    /// Wait for processing to complete
    pub wait: bool,
}

#[derive(Debug)]
pub struct ImportResult {
    pub root_uri: String,
    pub nodes_created: usize,
    pub processing_queued: usize,
}

pub struct ImportPipeline {
    #[allow(dead_code)]
    db: Arc<Database>,
    viking_fs: VikingFs,
    embedding_queue: EmbeddingQueue,
}

impl ImportPipeline {
    pub fn new(db: Arc<Database>, embedding_queue: EmbeddingQueue) -> Self {
        let viking_fs = VikingFs::new(Arc::clone(&db));
        Self {
            db,
            viking_fs,
            embedding_queue,
        }
    }

    pub fn embedding_queue(&self) -> &EmbeddingQueue {
        &self.embedding_queue
    }

    /// Import a file, URL, or directory.
    pub async fn import(
        &self,
        req: &ImportRequest,
        ctx: &RequestContext,
    ) -> Result<ImportResult, CoreError> {
        // Check if source is a local directory
        let path = std::path::Path::new(&req.source);
        if path.is_dir() {
            return self.import_directory(req, ctx).await;
        }

        let result = self.import_single_file(req, ctx).await?;

        if req.wait {
            self.embedding_queue.flush().await;
        }

        Ok(result)
    }

    /// Recursively import all files from a directory.
    async fn import_directory(
        &self,
        req: &ImportRequest,
        ctx: &RequestContext,
    ) -> Result<ImportResult, CoreError> {
        let dir = std::path::Path::new(&req.source);
        let dir_name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("dir");

        let root_name = req
            .target_uri
            .clone()
            .unwrap_or_else(|| format!("viking://resources/{}", slug_from_filename(dir_name)));
        let root_uri = VikingUri::parse(&root_name)?;

        self.viking_fs.mkdir(&root_uri, &ctx.owner)?;

        let mut total_nodes = 0;
        let mut total_queued = 0;
        let mut files_imported = 0;

        // Walk directory recursively
        for entry in walkdir(dir)? {
            let path = entry;
            if !path.is_file() {
                continue;
            }

            // Only import text-like files
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            if !is_importable_extension(ext) {
                continue;
            }

            // Build relative URI
            let relative = path
                .strip_prefix(dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let relative_clean = relative.trim_end_matches(&format!(".{ext}"));
            let file_uri_str = format!("{}/{}", root_uri.as_str(), relative_clean);

            // Import this file as a sub-resource
            let sub_req = ImportRequest {
                source: path.to_string_lossy().to_string(),
                target_uri: Some(file_uri_str),
                wait: false,
            };
            match self.import_single_file(&sub_req, ctx).await {
                Ok(result) => {
                    total_nodes += result.nodes_created;
                    total_queued += result.processing_queued;
                    files_imported += 1;
                    if files_imported % 50 == 0 {
                        tracing::info!(files_imported, total_nodes, "directory import progress");
                    }
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skipping file");
                }
            }
        }

        tracing::info!(files_imported, total_nodes, total_queued, "directory import complete");

        if req.wait {
            self.embedding_queue.flush().await;
        }

        Ok(ImportResult {
            root_uri: root_uri.as_str().to_string(),
            nodes_created: total_nodes,
            processing_queued: total_queued,
        })
    }

    /// Import a single file (not a directory). Used by both import() and import_directory().
    async fn import_single_file(
        &self,
        req: &ImportRequest,
        ctx: &RequestContext,
    ) -> Result<ImportResult, CoreError> {
        let (content, filename) = self.fetch(&req.source).await?;

        let root_name = req
            .target_uri
            .clone()
            .unwrap_or_else(|| format!("viking://resources/{}", slug_from_filename(&filename)));
        let root_uri = VikingUri::parse(&root_name)?;

        // Skip if content hasn't changed (incremental re-index)
        if !self.viking_fs.content_changed(&root_uri, &content)? {
            return Ok(ImportResult {
                root_uri: root_uri.as_str().to_string(),
                nodes_created: 0,
                processing_queued: 0,
            });
        }

        let nodes = parse_markdown(&content);
        self.viking_fs.mkdir(&root_uri, &ctx.owner)?;

        let mut nodes_created = 0;
        let mut processing_queued = 0;

        for node in &nodes {
            let child_uri = root_uri.child(&node.slug);
            self.viking_fs
                .write_context(&child_uri, "", "", true, &ctx.owner)?;
            self.viking_fs.write_content_raw(&child_uri, &node.text)?;
            nodes_created += 1;

            self.embedding_queue
                .enqueue(EmbeddingTask {
                    uri: child_uri.as_str().to_string(),
                    level: 0,
                    text: node.text.clone(),
                })
                .await?;
            processing_queued += 1;
        }

        let root_text = format!(
            "Directory containing {} sections: {}",
            nodes.len(),
            nodes
                .iter()
                .map(|n| n.slug.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        self.embedding_queue
            .enqueue(EmbeddingTask {
                uri: root_uri.as_str().to_string(),
                level: 0,
                text: root_text,
            })
            .await?;
        processing_queued += 1;

        // Store content hash for incremental re-index
        self.viking_fs.set_content_hash(&root_uri, &content)?;

        Ok(ImportResult {
            root_uri: root_uri.as_str().to_string(),
            nodes_created,
            processing_queued,
        })
    }

    async fn fetch(&self, source: &str) -> Result<(String, String), CoreError> {
        if source.starts_with("http://") || source.starts_with("https://") {
            let resp = reqwest::get(source)
                .await
                .map_err(|e| CoreError::Internal(format!("fetch URL failed: {e}")))?;

            if !resp.status().is_success() {
                return Err(CoreError::Internal(format!(
                    "fetch URL failed: HTTP {}",
                    resp.status()
                )));
            }

            let filename = source.rsplit('/').next().unwrap_or("resource").to_string();

            let content = resp
                .text()
                .await
                .map_err(|e| CoreError::Internal(format!("read URL body failed: {e}")))?;

            Ok((content, filename))
        } else {
            // Local file
            let content = std::fs::read_to_string(source)
                .map_err(|e| CoreError::Internal(format!("read file failed: {e}")))?;

            let filename = std::path::Path::new(source)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("resource")
                .to_string();

            Ok((content, filename))
        }
    }
}

/// Recursively walk a directory, returning all file paths.
fn walkdir(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, CoreError> {
    let mut files = Vec::new();
    walk_recursive(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk_recursive(
    dir: &std::path::Path,
    files: &mut Vec<std::path::PathBuf>,
) -> Result<(), CoreError> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| CoreError::Internal(format!("read dir {}: {e}", dir.display())))?;
    for entry in entries {
        let entry =
            entry.map_err(|e| CoreError::Internal(format!("dir entry: {e}")))?;
        let path = entry.path();
        if path.is_dir() {
            // Skip hidden dirs and common non-code dirs
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.')
                || name == "node_modules"
                || name == "target"
                || name == "__pycache__"
                || name == "vendor"
                || name == "dist"
                || name == "build"
            {
                continue;
            }
            walk_recursive(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

/// Check if a file extension is importable (text-based).
fn is_importable_extension(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "md" | "txt"
            | "rs"
            | "py"
            | "js"
            | "ts"
            | "tsx"
            | "jsx"
            | "go"
            | "java"
            | "c"
            | "h"
            | "cpp"
            | "hpp"
            | "cs"
            | "rb"
            | "php"
            | "swift"
            | "kt"
            | "scala"
            | "zig"
            | "lua"
            | "sh"
            | "bash"
            | "sql"
            | "yaml"
            | "yml"
            | "toml"
            | "json"
            | "xml"
            | "html"
            | "css"
            | "scss"
            | "vue"
            | "svelte"
            | "proto"
            | "rst"
    )
}

fn slug_from_filename(filename: &str) -> String {
    let stem = filename.rsplit('.').next_back().unwrap_or(filename);
    stem.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
