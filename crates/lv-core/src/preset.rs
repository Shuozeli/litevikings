use crate::uri::Scope;

/// A preset directory definition in the Viking filesystem.
pub struct DirectoryPreset {
    pub path: &'static str,
    pub abstract_text: &'static str,
    pub overview_text: &'static str,
    pub children: &'static [&'static DirectoryPreset],
}

pub fn preset_directories(scope: Scope) -> &'static DirectoryPreset {
    match scope {
        Scope::Session => &SESSION_PRESET,
        Scope::User => &USER_PRESET,
        Scope::Agent => &AGENT_PRESET,
        Scope::Resources => &RESOURCES_PRESET,
    }
}

static SESSION_PRESET: DirectoryPreset = DirectoryPreset {
    path: "",
    abstract_text: "Session scope. Stores complete context for a single conversation, including original messages and compressed summaries.",
    overview_text: "Session-level temporary data storage, can be archived or cleaned after session ends.",
    children: &[],
};

static USER_PRESET: DirectoryPreset = DirectoryPreset {
    path: "",
    abstract_text: "User scope. Stores user's long-term memory, persisted across sessions.",
    overview_text: "User-level persistent data storage for building user profiles and managing private memories.",
    children: &[&USER_MEMORIES],
};

static USER_MEMORIES: DirectoryPreset = DirectoryPreset {
    path: "memories",
    abstract_text: "User's long-term memory storage. Contains memory types like preferences, entities, events, managed hierarchically by type.",
    overview_text: "Use this directory to access user's personalized memories. Contains three main categories: 1) preferences-user preferences, 2) entities-entity memories, 3) events-event records.",
    children: &[&USER_PREFERENCES, &USER_ENTITIES, &USER_EVENTS],
};

static USER_PREFERENCES: DirectoryPreset = DirectoryPreset {
    path: "preferences",
    abstract_text: "User's personalized preference memories. Stores preferences by topic (communication style, code standards, domain interests, etc.), one subdirectory per preference type.",
    overview_text: "Access when adjusting output style, following user habits, or providing personalized services.",
    children: &[],
};

static USER_ENTITIES: DirectoryPreset = DirectoryPreset {
    path: "entities",
    abstract_text: "Entity memories from user's world. Each entity has its own subdirectory, including projects, people, concepts, etc.",
    overview_text: "Access when referencing user-related projects, people, concepts.",
    children: &[],
};

static USER_EVENTS: DirectoryPreset = DirectoryPreset {
    path: "events",
    abstract_text: "User's event records. Each event has its own subdirectory, recording important events, decisions, milestones, etc.",
    overview_text: "Access when reviewing user history, understanding event context, or tracking user progress.",
    children: &[],
};

static AGENT_PRESET: DirectoryPreset = DirectoryPreset {
    path: "",
    abstract_text: "Agent scope. Stores Agent's learning memories, instructions, and skills.",
    overview_text: "Agent-level global data storage. Contains three main categories: memories-learning memories, instructions-directives, skills-capability registry.",
    children: &[&AGENT_MEMORIES, &AGENT_INSTRUCTIONS, &AGENT_SKILLS],
};

static AGENT_MEMORIES: DirectoryPreset = DirectoryPreset {
    path: "memories",
    abstract_text: "Agent's long-term memory storage. Contains cases and patterns, managed hierarchically by type.",
    overview_text: "Use this directory to access Agent's learning memories. Contains two main categories: 1) cases-specific cases, 2) patterns-reusable patterns.",
    children: &[&AGENT_CASES, &AGENT_PATTERNS],
};

static AGENT_CASES: DirectoryPreset = DirectoryPreset {
    path: "cases",
    abstract_text: "Agent's case records. Stores specific problems and solutions encountered in each interaction.",
    overview_text: "Access cases when encountering similar problems, reference historical solutions.",
    children: &[],
};

static AGENT_PATTERNS: DirectoryPreset = DirectoryPreset {
    path: "patterns",
    abstract_text: "Agent's effective patterns. Stores reusable processes and best practices distilled from multiple interactions.",
    overview_text: "Access patterns when executing tasks requiring strategy selection or process determination.",
    children: &[],
};

static AGENT_INSTRUCTIONS: DirectoryPreset = DirectoryPreset {
    path: "instructions",
    abstract_text:
        "Agent instruction set. Contains Agent's behavioral directives, rules, and constraints.",
    overview_text: "Access when Agent needs to follow specific rules.",
    children: &[],
};

static AGENT_SKILLS: DirectoryPreset = DirectoryPreset {
    path: "skills",
    abstract_text: "Agent's skill registry. Flat storage of callable skill definitions.",
    overview_text: "Access when Agent needs to execute specific tasks. Skills categorized by tags.",
    children: &[],
};

static RESOURCES_PRESET: DirectoryPreset = DirectoryPreset {
    path: "",
    abstract_text: "Resources scope. Independent knowledge and resource storage, not bound to specific account or Agent.",
    overview_text: "Globally shared resource storage, organized by project/topic. No preset subdirectory structure, users create project directories as needed.",
    children: &[],
};
