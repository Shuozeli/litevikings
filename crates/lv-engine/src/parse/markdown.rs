/// Minimum section size (chars). Smaller sections get merged with previous.
const MIN_SECTION_SIZE: usize = 150;

/// Maximum heading level to split on. H1 and H2 create new nodes.
/// H3+ stay within the parent section.
const MAX_SPLIT_LEVEL: usize = 2;

/// Maximum nodes per file. If exceeded, merge smallest sections.
const MAX_NODES_PER_FILE: usize = 30;

/// A node in the parsed content tree.
#[derive(Debug, Clone)]
pub struct ContentNode {
    /// Slug for this node (used as URI segment)
    pub slug: String,
    /// Full text content of this section
    pub text: String,
    /// Children (subsections)
    pub children: Vec<ContentNode>,
}

/// Parse markdown text into a tree of ContentNodes, split by H1/H2 headings.
///
/// Strategy:
/// - Only split on H1 (# ) and H2 (## ) headings
/// - H3+ headings stay within their parent section (no separate node)
/// - Sections under MIN_SECTION_SIZE chars merge with previous section
/// - Total nodes capped at MAX_NODES_PER_FILE
pub fn parse_markdown(text: &str) -> Vec<ContentNode> {
    // Phase 1: Split on H1/H2 headings only
    let mut raw_sections: Vec<(String, String)> = Vec::new(); // (slug, text)
    let mut current_text = String::new();
    let mut current_slug = String::new();

    for line in text.lines() {
        if let Some(heading) = parse_heading(line) {
            if heading.level <= MAX_SPLIT_LEVEL {
                // Save previous section
                let trimmed = current_text.trim().to_string();
                if !trimmed.is_empty() || !current_slug.is_empty() {
                    raw_sections.push((
                        if current_slug.is_empty() {
                            "root".to_string()
                        } else {
                            current_slug.clone()
                        },
                        trimmed,
                    ));
                }
                current_slug = slugify(&heading.title);
                current_text = format!("{line}\n");
                continue;
            }
        }
        // H3+ headings and regular text: append to current section
        current_text.push_str(line);
        current_text.push('\n');
    }

    // Don't forget the last section
    let trimmed = current_text.trim().to_string();
    if !trimmed.is_empty() {
        raw_sections.push((
            if current_slug.is_empty() {
                "root".to_string()
            } else {
                current_slug
            },
            trimmed,
        ));
    }

    // Phase 2: Merge small sections with previous
    let mut merged: Vec<(String, String)> = Vec::new();
    for (slug, text) in raw_sections {
        if text.len() < MIN_SECTION_SIZE && !merged.is_empty() {
            // Merge with previous section
            let prev = merged.last_mut().unwrap();
            prev.1.push_str("\n\n");
            prev.1.push_str(&text);
        } else {
            merged.push((slug, text));
        }
    }

    // Phase 3: If still too many nodes, merge smallest pairs
    while merged.len() > MAX_NODES_PER_FILE {
        // Find the smallest section (excluding first)
        let min_idx = merged
            .iter()
            .enumerate()
            .skip(1) // Don't merge the root
            .min_by_key(|(_, (_, text))| text.len())
            .map(|(i, _)| i)
            .unwrap_or(1);

        // Merge with previous
        if min_idx > 0 {
            let (_, removed_text) = merged.remove(min_idx);
            let prev = &mut merged[min_idx - 1];
            prev.1.push_str("\n\n");
            prev.1.push_str(&removed_text);
        } else {
            break;
        }
    }

    // Phase 4: Build ContentNodes
    let mut nodes: Vec<ContentNode> = merged
        .into_iter()
        .map(|(slug, text)| ContentNode {
            slug,
            text,
            children: vec![],
        })
        .collect();

    deduplicate_slugs(&mut nodes);
    nodes
}

struct Heading {
    level: usize,
    title: String,
}

fn parse_heading(line: &str) -> Option<Heading> {
    let trimmed = line.trim();
    if !trimmed.starts_with('#') {
        return None;
    }
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let title = trimmed[hashes..].trim().to_string();
    if title.is_empty() {
        return None;
    }
    Some(Heading {
        level: hashes,
        title,
    })
}

fn slugify(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
        .chars()
        .take(60)
        .collect()
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
    fn parse_simple_markdown() {
        let md = r#"# Title

Some intro text that is long enough to not be merged with the next section because we need at least 300 characters of content to avoid being merged. This is filler text to ensure the section passes the minimum size threshold.

## Section One

Content of section one which is also long enough to stand on its own as a separate section node. We need at least 300 characters so let me add some more descriptive text about what section one covers.

## Section Two

Content of section two with enough detail to be its own node. This section covers important topics that deserve their own embedding vector in the search index.

### Subsection (stays with Section Two)

Nested content that remains part of Section Two because we only split on H1/H2.
"#;
        let nodes = parse_markdown(md);
        assert_eq!(nodes.len(), 3); // Title + Section One + Section Two (with subsection)
        assert_eq!(nodes[0].slug, "title");
        assert!(nodes[0].text.contains("Some intro text"));
        assert_eq!(nodes[1].slug, "section_one");
        assert_eq!(nodes[2].slug, "section_two");
        // H3 subsection stays inside Section Two
        assert!(nodes[2].text.contains("Subsection"));
        assert!(nodes[2].text.contains("Nested content"));
    }

    #[test]
    fn small_sections_merge() {
        let md = "## Big Section\n\nLots of content here that is definitely long enough to stand on its own as a section. We need at least 300 characters of content so this needs to be quite verbose and descriptive.\n\n## Tiny\n\nSmall.\n\n## Another Big\n\nMore substantial content that also exceeds the minimum threshold for standing on its own as a separate section node in the parsed tree.\n";
        let nodes = parse_markdown(md);
        // "Tiny" (6 chars) should merge with "Big Section"
        assert!(nodes.len() <= 3);
        // Check that "Small." ended up in the big section
        let combined = nodes
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(combined.contains("Small."));
    }

    #[test]
    fn parse_no_headings() {
        let md = "Just plain text\nwith multiple lines\n";
        let nodes = parse_markdown(md);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].slug, "root");
    }

    #[test]
    fn h3_does_not_split() {
        let md = "## Main Section\n\nIntro text with enough content to be a standalone section. We need at least 150 characters of descriptive text here so this section is not merged with any adjacent section in the parser.\n\n### Sub A\n\nSub A content.\n\n### Sub B\n\nSub B content.\n\n## Next Section\n\nAlso enough content to stand alone as a separate section with well over 150 characters of text describing what this section covers in detail.\n";
        let nodes = parse_markdown(md);
        // Should be 2 nodes: Main Section (with Sub A, Sub B inside) + Next Section
        assert_eq!(nodes.len(), 2);
        assert!(nodes[0].text.contains("Sub A content"));
        assert!(nodes[0].text.contains("Sub B content"));
    }

    #[test]
    fn max_nodes_cap() {
        // Create 50 H2 sections -- should be capped at MAX_NODES_PER_FILE
        let mut md = String::new();
        for i in 0..50 {
            md.push_str(&format!(
                "## Section {i}\n\nContent for section {i} with enough text to be substantial. This is filler content to ensure each section has reasonable size for testing the max nodes cap. Adding more words to reach the minimum threshold.\n\n"
            ));
        }
        let nodes = parse_markdown(&md);
        assert!(
            nodes.len() <= MAX_NODES_PER_FILE,
            "got {} nodes, expected <= {}",
            nodes.len(),
            MAX_NODES_PER_FILE
        );
    }

    #[test]
    fn api_reference_does_not_explode() {
        // Simulate a C API reference with many H4+ sub-headings
        let mut md = String::from("## API Reference\n\nOverview of the API with enough intro text to be a standalone section. This covers all the functions available in the C API.\n\n");
        for i in 0..100 {
            md.push_str(&format!("#### `function_{i}`\n\nDoes thing {i}.\n\n##### Parameters\n\n* `p`: param\n\n##### Return Value\n\nThe result.\n\n"));
        }
        let nodes = parse_markdown(&md);
        // All 100 H4 functions should stay inside the single H2 "API Reference" node
        assert_eq!(nodes.len(), 1, "got {} nodes, expected 1", nodes.len());
        assert!(nodes[0].text.contains("function_99"));
    }

    #[test]
    fn slugify_heading() {
        assert_eq!(slugify("Hello World"), "hello_world");
        assert_eq!(slugify("  Spaces  and---dashes  "), "spaces_and_dashes");
        assert_eq!(slugify("Quick Start (v2)"), "quick_start_v2");
    }

    #[test]
    fn deduplicate_same_slugs() {
        let md = "## Section\n\nFirst section content that is definitely long enough to stand on its own as a node. Adding more text to reach the minimum section size threshold. This should be well over 150 characters now.\n\n## Section\n\nSecond section with different content but same heading. Also needs to be long enough to avoid being merged with the first. More text to pad it to the required minimum length for testing.\n\n## Section\n\nThird section again with the same heading. Still needs to meet the minimum size requirement for the deduplication test to work correctly as intended.\n\n";
        let nodes = parse_markdown(md);
        let slugs: Vec<&str> = nodes.iter().map(|n| n.slug.as_str()).collect();
        assert_eq!(slugs, vec!["section", "section_1", "section_2"]);
    }
}
