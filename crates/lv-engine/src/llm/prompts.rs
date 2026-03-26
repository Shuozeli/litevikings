/// Generate L0 abstract (~100 tokens) from content.
pub const GENERATE_ABSTRACT: &str = r#"You are a summarization expert. Given the following content, produce an ultra-concise abstract in 1-2 sentences (under 100 tokens). The abstract should capture the essence of what this content is about, optimized for semantic search retrieval.

Content:
{content}

Abstract:"#;

/// Generate L1 overview (~1000 tokens) from content.
pub const GENERATE_OVERVIEW: &str = r#"You are a documentation expert. Given the following content, produce a comprehensive overview (under 1000 tokens) that serves as a navigation guide. Include:
- What this content covers
- Key topics and concepts
- How it relates to its context
- When an agent should load the full content (L2)

Content:
{content}

Overview:"#;

/// Compress session messages into a summary.
pub const COMPRESS_SESSION: &str = r#"Compress the following conversation into a concise summary preserving:
- Key decisions made
- Important facts discussed
- Action items
- Context references used

Conversation:
{messages}

Compressed summary:"#;

/// Extract memories from conversation.
pub const EXTRACT_MEMORIES: &str = r#"Analyze the following conversation and extract structured memories. Categorize each as:
- preference: User preference or habit
- entity: Person, project, concept, or thing
- event: Decision, milestone, or incident

For each memory, provide:
- category: preference | entity | event
- title: Brief descriptive title
- content: The memory content

Conversation:
{messages}

Respond as JSON array:
[{"category": "...", "title": "...", "content": "..."}]"#;

/// Generate abstract for a directory based on its children.
pub const DIRECTORY_ABSTRACT: &str = r#"Given the following directory contents (child abstracts), generate an abstract for this directory that summarizes what it contains.

Directory: {uri}
Children:
{children_abstracts}

Directory abstract:"#;
