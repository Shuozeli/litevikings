use tree_sitter::{Language, Node, Parser};

use super::markdown::ContentNode;

/// Supported languages for AST-based code parsing.
#[derive(Debug, Clone, Copy)]
enum CodeLanguage {
    C,
    Python,
    JavaScript,
    TypeScript,
    Rust,
    Go,
}

/// Detect language from file extension.
fn detect_language(filename: &str) -> Option<CodeLanguage> {
    let ext = filename.rsplit('.').next()?.to_lowercase();
    match ext.as_str() {
        "c" | "h" => Some(CodeLanguage::C),
        "py" => Some(CodeLanguage::Python),
        "js" | "jsx" | "mjs" => Some(CodeLanguage::JavaScript),
        "ts" | "tsx" => Some(CodeLanguage::TypeScript),
        "rs" => Some(CodeLanguage::Rust),
        "go" => Some(CodeLanguage::Go),
        _ => None,
    }
}

/// Get tree-sitter Language for a CodeLanguage.
fn get_ts_language(lang: CodeLanguage) -> Language {
    match lang {
        CodeLanguage::C => tree_sitter_c::LANGUAGE.into(),
        CodeLanguage::Python => tree_sitter_python::LANGUAGE.into(),
        CodeLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        CodeLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        CodeLanguage::Rust => tree_sitter_rust::LANGUAGE.into(),
        CodeLanguage::Go => tree_sitter_go::LANGUAGE.into(),
    }
}

/// Node types that represent function/method definitions per language.
fn function_node_types(lang: CodeLanguage) -> &'static [&'static str] {
    match lang {
        CodeLanguage::C => &["function_definition", "declaration"],
        CodeLanguage::Python => &["function_definition", "class_definition"],
        CodeLanguage::JavaScript => &[
            "function_declaration",
            "method_definition",
            "class_declaration",
            "export_statement",
        ],
        CodeLanguage::TypeScript => &[
            "function_declaration",
            "method_definition",
            "class_declaration",
            "interface_declaration",
            "type_alias_declaration",
            "export_statement",
        ],
        CodeLanguage::Rust => &[
            "function_item",
            "impl_item",
            "struct_item",
            "enum_item",
            "trait_item",
            "type_item",
        ],
        CodeLanguage::Go => &[
            "function_declaration",
            "method_declaration",
            "type_declaration",
        ],
    }
}

/// Try to parse source code into ContentNodes using tree-sitter AST.
/// Returns None if the language is unsupported or parsing fails.
/// Falls back to the markdown/text parser in that case.
pub fn parse_code(filename: &str, source: &str) -> Option<Vec<ContentNode>> {
    let lang = detect_language(filename)?;
    let ts_lang = get_ts_language(lang);
    let fn_types = function_node_types(lang);

    let mut parser = Parser::new();
    parser.set_language(&ts_lang).ok()?;

    let tree = parser.parse(source, None)?;
    let root = tree.root_node();

    let mut nodes = Vec::new();
    let mut seen_ranges: Vec<(usize, usize)> = Vec::new();

    // Walk top-level children looking for function/class definitions
    extract_definitions(root, source, fn_types, &mut nodes, &mut seen_ranges, 0);

    // If we found very few definitions, the file might be mostly top-level code
    // (e.g., a Python script or C file with lots of macros). Add uncovered regions.
    if nodes.is_empty() {
        return None; // Fall back to text parser
    }

    // Add any significant uncovered regions as "module-level" nodes
    add_uncovered_regions(source, &seen_ranges, &mut nodes);

    // Cap at 30 nodes like the markdown parser
    while nodes.len() > 30 {
        // Merge smallest into previous
        let min_idx = nodes
            .iter()
            .enumerate()
            .skip(1)
            .min_by_key(|(_, n)| n.text.len())
            .map(|(i, _)| i)
            .unwrap_or(1);
        if min_idx > 0 {
            let removed = nodes.remove(min_idx);
            nodes[min_idx - 1].text.push_str("\n\n");
            nodes[min_idx - 1].text.push_str(&removed.text);
        } else {
            break;
        }
    }

    // Deduplicate slugs
    deduplicate_slugs(&mut nodes);

    Some(nodes)
}

/// Recursively extract function/class definitions from AST.
fn extract_definitions(
    node: Node,
    source: &str,
    fn_types: &[&str],
    nodes: &mut Vec<ContentNode>,
    seen_ranges: &mut Vec<(usize, usize)>,
    depth: usize,
) {
    // Don't recurse too deep
    if depth > 3 {
        return;
    }

    for child in node.children(&mut node.walk()) {
        let kind = child.kind();

        if fn_types.contains(&kind) {
            let start = child.start_byte();
            let end = child.end_byte();
            let text = &source[start..end];

            // Include any comment immediately before this node
            let comment = get_leading_comment(source, start);
            let full_text = if comment.is_empty() {
                text.to_string()
            } else {
                format!("{comment}\n{text}")
            };

            // Generate slug from the first line (usually the function signature)
            let first_line = text.lines().next().unwrap_or(kind);
            let slug = slugify_code(first_line);

            nodes.push(ContentNode {
                slug,
                text: full_text,
                children: vec![],
            });
            seen_ranges.push((start, end));
        } else {
            // Recurse into containers (e.g., module, namespace, impl block)
            extract_definitions(child, source, fn_types, nodes, seen_ranges, depth + 1);
        }
    }
}

/// Get the comment block immediately before a byte offset.
fn get_leading_comment(source: &str, start: usize) -> String {
    let before = &source[..start];
    let trimmed = before.trim_end();

    // Walk backwards collecting comment lines
    let mut comment_lines: Vec<&str> = Vec::new();
    for line in trimmed.lines().rev() {
        let stripped = line.trim();
        if stripped.starts_with("//")
            || stripped.starts_with("/*")
            || stripped.starts_with('*')
            || stripped.starts_with('#')
            || stripped.starts_with("///")
            || stripped.starts_with("\"\"\"")
        {
            comment_lines.push(line);
        } else if stripped.is_empty() && !comment_lines.is_empty() {
            // Allow one blank line between comment and definition
            continue;
        } else {
            break;
        }
    }

    comment_lines.reverse();
    comment_lines.join("\n")
}

/// Add uncovered regions (module-level code not inside any definition).
fn add_uncovered_regions(
    source: &str,
    seen_ranges: &[(usize, usize)],
    nodes: &mut Vec<ContentNode>,
) {
    let mut sorted = seen_ranges.to_vec();
    sorted.sort_by_key(|r| r.0);

    let mut last_end = 0;
    for (start, end) in &sorted {
        if *start > last_end {
            let gap = source[last_end..*start].trim();
            if gap.len() > 150 {
                nodes.insert(
                    0,
                    ContentNode {
                        slug: "module_level".to_string(),
                        text: gap.to_string(),
                        children: vec![],
                    },
                );
            }
        }
        last_end = last_end.max(*end);
    }

    // Trailing code after last definition
    if last_end < source.len() {
        let gap = source[last_end..].trim();
        if gap.len() > 150 {
            nodes.push(ContentNode {
                slug: "module_tail".to_string(),
                text: gap.to_string(),
                children: vec![],
            });
        }
    }
}

/// Generate a slug from a code line (function signature).
fn slugify_code(line: &str) -> String {
    // Extract function/method name from common patterns
    let cleaned = line
        .trim()
        .trim_start_matches("pub ")
        .trim_start_matches("async ")
        .trim_start_matches("fn ")
        .trim_start_matches("func ")
        .trim_start_matches("def ")
        .trim_start_matches("function ")
        .trim_start_matches("class ")
        .trim_start_matches("struct ")
        .trim_start_matches("enum ")
        .trim_start_matches("trait ")
        .trim_start_matches("impl ")
        .trim_start_matches("interface ")
        .trim_start_matches("type ")
        .trim_start_matches("export ")
        .trim_start_matches("static ")
        .trim_start_matches("const ")
        .trim_start_matches("void ")
        .trim_start_matches("int ")
        .trim_start_matches("char ")
        .trim_start_matches("unsigned ")
        .trim_start_matches("signed ");

    // Take until first '(' or '{' or ':' or ' '
    let name: String = cleaned
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();

    if name.is_empty() {
        "unnamed".to_string()
    } else {
        name.chars().take(60).collect()
    }
}

fn deduplicate_slugs(nodes: &mut [ContentNode]) {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for node in nodes.iter_mut() {
        let count = seen.entry(node.slug.clone()).or_insert(0);
        if *count > 0 {
            node.slug = format!("{}_{}", node.slug, count);
        }
        *count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_c_functions() {
        let source = r#"
#include <stdio.h>

// Initialize the database connection
int sqlite3_open(const char *filename, sqlite3 **ppDb) {
    // implementation
    return 0;
}

// Close the database
int sqlite3_close(sqlite3 *db) {
    return 0;
}
"#;
        let nodes = parse_code("test.c", source).unwrap();
        assert!(nodes.len() >= 2, "got {} nodes", nodes.len());
        // Should find sqlite3_open and sqlite3_close
        let slugs: Vec<&str> = nodes.iter().map(|n| n.slug.as_str()).collect();
        assert!(
            slugs.iter().any(|s| s.contains("sqlite3_open")),
            "missing sqlite3_open in {slugs:?}"
        );
    }

    #[test]
    fn parse_python_functions() {
        let source = r#"
import os

class Database:
    """A database connection wrapper."""

    def __init__(self, path):
        self.path = path

    def query(self, sql):
        """Execute a SQL query."""
        pass

def connect(url):
    """Create a new connection."""
    return Database(url)
"#;
        let nodes = parse_code("test.py", source).unwrap();
        assert!(!nodes.is_empty());
        let slugs: Vec<&str> = nodes.iter().map(|n| n.slug.as_str()).collect();
        assert!(
            slugs
                .iter()
                .any(|s| s.contains("Database") || s.contains("connect")),
            "missing expected definitions in {slugs:?}"
        );
    }

    #[test]
    fn parse_rust_functions() {
        let source = r#"
/// A Viking URI with validated components.
pub struct VikingUri {
    raw: String,
}

impl VikingUri {
    /// Parse a raw URI string.
    pub fn parse(raw: &str) -> Result<Self, Error> {
        todo!()
    }
}

/// Run the server.
pub async fn serve(config: Config) -> Result<()> {
    todo!()
}
"#;
        let nodes = parse_code("test.rs", source).unwrap();
        assert!(!nodes.is_empty());
    }

    #[test]
    fn unsupported_language_returns_none() {
        assert!(parse_code("test.zig", "const std = @import(\"std\");").is_none());
        assert!(parse_code("test.rb", "class Foo; end").is_none());
    }

    #[test]
    fn caps_at_30_nodes() {
        // Generate a C file with 50 functions
        let mut source = String::new();
        for i in 0..50 {
            source.push_str(&format!("int func_{i}() {{ return {i}; }}\n"));
        }
        let nodes = parse_code("big.c", &source).unwrap();
        assert!(
            nodes.len() <= 30,
            "got {} nodes, expected <= 30",
            nodes.len()
        );
    }
}
