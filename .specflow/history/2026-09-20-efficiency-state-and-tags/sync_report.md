# Sync Report

## persisted-state
- ADDED Persist user state in isolated SQLite database
- ADDED Ensure identity key stability across engines
- ADDED Tolerate unknown engine keys and preserve records
- ADDED Execute schema migrations and degrade safely
- ADDED Manage tag normalization and orphan records

## session-catalog
- ADDED Encapsulate engine knowledge and boundary governance
- MODIFIED Discover sessions for the active engine

## tui-interaction
- ADDED Toggle favorite status with keyboard shortcut
- ADDED Edit tags via dedicated modal dialog
- ADDED Filter sessions using search prefixes
- MODIFIED Navigate engines and session trees
- MODIFIED Support project and time grouping
