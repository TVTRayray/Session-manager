# Sync Report

## content-search
- ADDED Execute asynchronous background indexing with dedicated database connections
- ADDED Index text transcript blocks while excluding auxiliary metadata
- ADDED Safely sanitize user input and query full-text index with ranking and snippets
- ADDED Navigate from search hit to session detail and handle stale index

## persisted-state
- ADDED Migrate schema to v2 with FTS5 virtual table and index metadata
- ADDED Manage full-text search index records and lifecycle synchronization

## session-export
- ADDED Generate deterministic Markdown representations of session transcripts
- ADDED Enforce directory containment and sanitize filenames
- ADDED Execute batch group exports with manifest index generation

## tui-interaction
- ADDED Export single sessions and groups to Markdown with keyboard shortcut
- ADDED Search full-text content via interactive overlay
- ADDED Display project metrics and recent sessions in Project Dashboard
