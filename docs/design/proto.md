# lv-proto: Protobuf Service Definitions

## Overview

`lv-proto` defines the gRPC service contracts between the CLI and server. These are the
**single source of truth** for the API. The HTTP/JSON gateway is auto-generated from
these same definitions for backward compatibility with the upstream Python SDK.

## Crate Structure

```
lv-proto/
  proto/
    litevikings/v1/
      filesystem.proto
      resources.proto
      sessions.proto
      search.proto
      relations.proto
      admin.proto
      common.proto         -- shared types
  src/
    lib.rs                 -- re-exports generated code
  build.rs                 -- tonic-build
```

## Shared Types (`common.proto`)

```protobuf
syntax = "proto3";
package litevikings.v1;

message Owner {
  string account_id = 1;
  string user_id = 2;
  optional string agent_name = 3;
}

message RequestContext {
  Owner owner = 1;
  string role = 2;  // "root", "user", "agent"
}

message Context {
  string id = 1;
  string uri = 2;
  optional string parent_uri = 3;
  optional int32 level = 4;      // 0=Abstract, 1=Overview, 2=Detail
  bool is_leaf = 5;
  string context_type = 6;       // "skill", "memory", "resource"
  string category = 7;
  string abstract_text = 8;
  string owner_account = 9;
  string owner_user = 10;
  optional string owner_agent = 11;
  optional string session_id = 12;
  int64 active_count = 13;
  string meta = 14;              // JSON string
  repeated float vector = 15;
  string created_at = 16;        // ISO 8601
  string updated_at = 17;
}

message Relation {
  string id = 1;
  repeated string uris = 2;
  string reason = 3;
  string created_at = 4;
}

// Standard response wrapper (mirrors upstream JSON envelope)
message StatusResponse {
  string status = 1;  // "ok" or "error"
  optional ErrorInfo error = 2;
}

message ErrorInfo {
  string code = 1;
  string message = 2;
}
```

## Filesystem Service (`filesystem.proto`)

```protobuf
syntax = "proto3";
package litevikings.v1;

import "litevikings/v1/common.proto";

service FilesystemService {
  rpc Ls (LsRequest) returns (LsResponse);
  rpc Tree (TreeRequest) returns (TreeResponse);
  rpc Stat (StatRequest) returns (StatResponse);
  rpc Mkdir (MkdirRequest) returns (StatusResponse);
  rpc Rm (RmRequest) returns (StatusResponse);
  rpc Mv (MvRequest) returns (StatusResponse);
  rpc Read (ReadRequest) returns (ReadResponse);
  rpc ReadAbstract (ReadAbstractRequest) returns (ReadAbstractResponse);
  rpc ReadOverview (ReadOverviewRequest) returns (ReadOverviewResponse);
  rpc Write (WriteRequest) returns (StatusResponse);
}

message DirEntry {
  string uri = 1;
  bool is_leaf = 2;
  string abstract_text = 3;
  string context_type = 4;
  string updated_at = 5;
}

message TreeNode {
  string uri = 1;
  bool is_leaf = 2;
  string abstract_text = 3;
  string context_type = 4;
  int32 depth = 5;
  repeated TreeNode children = 6;
}

message LsRequest {
  string uri = 1;
  bool simple = 2;
  bool recursive = 3;
  string output = 4;          // "agent" or "original"
  int32 abs_limit = 5;
  bool show_all_hidden = 6;
  int32 node_limit = 7;
}

message LsResponse {
  repeated DirEntry entries = 1;
}

message TreeRequest {
  string uri = 1;
  string output = 2;
  int32 abs_limit = 3;
  bool show_all_hidden = 4;
  int32 node_limit = 5;
  int32 level_limit = 6;
}

message TreeResponse {
  TreeNode root = 1;
}

message StatRequest {
  string uri = 1;
}

message StatResponse {
  string uri = 1;
  bool is_leaf = 2;
  string context_type = 3;
  string abstract_text = 4;
  int64 child_count = 5;
  string created_at = 6;
  string updated_at = 7;
}

message MkdirRequest {
  string uri = 1;
}

message RmRequest {
  string uri = 1;
  bool recursive = 2;
}

message MvRequest {
  string from = 1;
  string to = 2;
}

message ReadRequest {
  string uri = 1;
}

message ReadResponse {
  string content = 1;
}

message ReadAbstractRequest {
  string uri = 1;
}

message ReadAbstractResponse {
  string abstract_text = 1;
}

message ReadOverviewRequest {
  string uri = 1;
}

message ReadOverviewResponse {
  string overview = 1;
}

message WriteRequest {
  string uri = 1;
  string content = 2;
}
```

## Resources Service (`resources.proto`)

```protobuf
syntax = "proto3";
package litevikings.v1;

import "litevikings/v1/common.proto";

service ResourcesService {
  rpc AddResource (AddResourceRequest) returns (AddResourceResponse);
  rpc AddSkill (AddSkillRequest) returns (StatusResponse);
  rpc WaitProcessed (WaitProcessedRequest) returns (StatusResponse);
}

message AddResourceRequest {
  optional string path = 1;
  optional string temp_path = 2;
  optional string to = 3;
  optional string parent = 4;
  string reason = 5;
  string instruction = 6;
  bool wait = 7;
  optional float timeout = 8;
  bool strict = 9;
  optional string ignore_dirs = 10;
  optional string include = 11;
  optional string exclude = 12;
  bool directly_upload_media = 13;
  optional bool preserve_structure = 14;
  float watch_interval = 15;
}

message AddResourceResponse {
  string root_uri = 1;
  int64 nodes_created = 2;
  int64 processing_queued = 3;
}

message AddSkillRequest {
  string uri = 1;
  string name = 2;
  string description = 3;
  string content = 4;
  repeated string tags = 5;
}

message WaitProcessedRequest {
  optional float timeout = 1;
}
```

## Sessions Service (`sessions.proto`)

```protobuf
syntax = "proto3";
package litevikings.v1;

import "litevikings/v1/common.proto";

service SessionsService {
  rpc Create (CreateSessionRequest) returns (CreateSessionResponse);
  rpc Get (GetSessionRequest) returns (GetSessionResponse);
  rpc Delete (DeleteSessionRequest) returns (StatusResponse);
  rpc List (ListSessionsRequest) returns (ListSessionsResponse);
  rpc AddMessage (AddMessageRequest) returns (StatusResponse);
  rpc GetMessages (GetMessagesRequest) returns (GetMessagesResponse);
  rpc Commit (CommitRequest) returns (CommitResponse);
  rpc RecordUsage (RecordUsageRequest) returns (StatusResponse);
}

message CreateSessionRequest {
  optional string session_id = 1;
}

message CreateSessionResponse {
  string session_id = 1;
  string session_uri = 2;
}

// --- Message parts (matching upstream) ---

message TextPart {
  string text = 1;
}

message ContextPart {
  string uri = 1;
  string context_type = 2;    // "memory", "resource", "skill"
  string abstract_text = 3;
}

message ToolPart {
  string tool_id = 1;
  string tool_name = 2;
  string tool_uri = 3;
  string skill_uri = 4;
  optional string tool_input = 5;  // JSON string
  string tool_output = 6;
  string tool_status = 7;          // "pending", "success", "error"
}

message MessagePart {
  oneof part {
    TextPart text = 1;
    ContextPart context = 2;
    ToolPart tool = 3;
  }
}

message Message {
  string role = 1;
  repeated MessagePart parts = 2;
  string timestamp = 3;
}

// --- Requests ---

message GetSessionRequest {
  string session_id = 1;
}

message GetSessionResponse {
  string session_id = 1;
  string session_uri = 2;
  string owner_user = 3;
  string compression = 4;  // JSON
  string stats = 5;        // JSON
  string created_at = 6;
}

message DeleteSessionRequest {
  string session_id = 1;
}

message ListSessionsRequest {}

message ListSessionsResponse {
  repeated GetSessionResponse sessions = 1;
}

message AddMessageRequest {
  string session_id = 1;
  string role = 2;
  optional string content = 3;           // simple mode
  repeated MessagePart parts = 4;        // parts mode (takes precedence)
}

message GetMessagesRequest {
  string session_id = 1;
}

message GetMessagesResponse {
  repeated Message messages = 1;
}

message CommitRequest {
  string session_id = 1;
}

message CommitResponse {
  int64 memories_extracted = 1;
}

message RecordUsageRequest {
  string session_id = 1;
  string uri = 2;
  string usage_type = 3;     // "context", "skill"
  float contribution = 4;
  string input = 5;
  string output = 6;
  bool success = 7;
}
```

## Search Service (`search.proto`)

```protobuf
syntax = "proto3";
package litevikings.v1;

import "litevikings/v1/common.proto";

service SearchService {
  rpc Find (FindRequest) returns (FindResponse);
  rpc Search (SearchRequest) returns (FindResponse);  // same response shape
  rpc Grep (GrepRequest) returns (GrepResponse);
  rpc Glob (GlobRequest) returns (GlobResponse);
}

message MatchedContext {
  string uri = 1;
  string context_type = 2;
  int32 level = 3;
  string abstract_text = 4;
  float score = 5;
  repeated RelatedContext related = 6;
}

message RelatedContext {
  string uri = 1;
  string reason = 2;
  string abstract_text = 3;
}

message FindRequest {
  string query = 1;
  optional string target_uri = 2;
  int32 limit = 3;
  optional int32 node_limit = 4;
  optional float score_threshold = 5;
  optional string filter = 6;  // JSON
}

message SearchRequest {
  string query = 1;
  optional string target_uri = 2;
  optional string session_id = 3;
  int32 limit = 4;
  optional int32 node_limit = 5;
  optional float score_threshold = 6;
  optional string filter = 7;  // JSON
}

message FindResponse {
  string query = 1;
  repeated MatchedContext resources = 2;
  int64 total_searched = 3;
  int32 rounds = 4;
}

message GrepRequest {
  string uri = 1;
  string pattern = 2;
  bool case_insensitive = 3;
  optional int32 node_limit = 4;
}

message GrepMatch {
  string uri = 1;
  string line = 2;
  int32 line_number = 3;
}

message GrepResponse {
  repeated GrepMatch matches = 1;
}

message GlobRequest {
  string pattern = 1;
  string uri = 2;
  optional int32 node_limit = 3;
}

message GlobResponse {
  repeated string uris = 1;
}
```

## Relations Service (`relations.proto`)

```protobuf
syntax = "proto3";
package litevikings.v1;

import "litevikings/v1/common.proto";

service RelationsService {
  rpc GetRelations (GetRelationsRequest) returns (GetRelationsResponse);
  rpc Link (LinkRequest) returns (LinkResponse);
  rpc Unlink (UnlinkRequest) returns (StatusResponse);
}

message GetRelationsRequest {
  string uri = 1;
}

message GetRelationsResponse {
  repeated Relation relations = 1;
}

message LinkRequest {
  repeated string uris = 1;
  string reason = 2;
}

message LinkResponse {
  Relation relation = 1;
}

message UnlinkRequest {
  string uri = 1;
  string relation_id = 2;
}
```

## Admin Service (`admin.proto`)

```protobuf
syntax = "proto3";
package litevikings.v1;

import "litevikings/v1/common.proto";

service AdminService {
  rpc Initialize (InitializeRequest) returns (InitializeResponse);
  rpc Status (StatusRequest) returns (SystemStatusResponse);
}

message InitializeRequest {}

message InitializeResponse {
  int64 directories_created = 1;
}

message StatusRequest {}

message SystemStatusResponse {
  int64 context_count = 1;
  int64 session_count = 2;
  int64 vector_count = 3;
  int64 db_size_bytes = 4;
}
```

## Build Configuration

### `build.rs`

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                "proto/litevikings/v1/common.proto",
                "proto/litevikings/v1/filesystem.proto",
                "proto/litevikings/v1/resources.proto",
                "proto/litevikings/v1/sessions.proto",
                "proto/litevikings/v1/search.proto",
                "proto/litevikings/v1/relations.proto",
                "proto/litevikings/v1/admin.proto",
            ],
            &["proto"],
        )?;
    Ok(())
}
```

### `Cargo.toml`

```toml
[dependencies]
tonic = { workspace = true }
prost = { workspace = true }

[build-dependencies]
tonic-build = { workspace = true }
```

## Authentication

gRPC metadata (headers) carry auth info:

- `x-api-key`: API key string
- `x-user-id`: User identifier
- `x-account-id`: Account/tenant identifier
- `x-agent-name`: Agent name (optional)

A tonic interceptor extracts these into `RequestContext` before each handler.

## Design Decisions

### Why proto files instead of just Rust traits?

1. **Single source of truth** -- protobuf generates both server and client code.
   No drift between what the server exposes and what the CLI calls.
2. **HTTP gateway** -- tools like `tonic-web` or a custom axum layer can
   auto-translate REST/JSON to gRPC using the same proto definitions.
3. **Language-agnostic** -- if someone wants a Python or Go client later, they
   generate from the same protos.
4. **Streaming-ready** -- gRPC supports server streaming (useful for future
   `WaitProcessed` progress updates or real-time session events).

### Field mapping to upstream JSON API

The proto messages mirror upstream's JSON request/response shapes exactly.
Field names use snake_case in proto (standard) and the HTTP gateway translates
to camelCase for JSON wire format where needed (matching upstream conventions).
