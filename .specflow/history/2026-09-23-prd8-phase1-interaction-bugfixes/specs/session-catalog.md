# Delta Spec: session-catalog

## ADDED Requirements

### Requirement: Link tintinweb subagents to parent sessions

The system MUST identify subagent sessions created by the `@tintinweb/pi-subagents` extension after native Pi subagent relationship processing completes. It MUST parse the subagent identity from the child session naming pattern (`<AgentType>#<first-8-of-agentId>`) within `session_info` records and match it with corresponding `toolResult` records (for tools named `"Agent"` or `"subagent"`) in the parent session transcript. The matching logic MUST inspect parent transcript evidence and working directory context using dual sources—comparing both runtime/recorder working directories (`recorder.cwd` / transcript `cwd`) and session file header contexts (`parentSession` and session header `cwd`)—to reliably disambiguate forked or copied sessions. Pre-existing native Pi subagent relationships MUST NOT be overwritten, and unmatchable or ambiguous subagents MUST remain visible as detached root sessions without error or crash.

#### Scenario: Identify and link tintinweb subagent to parent session

- **GIVEN** a Pi session directory contains a parent session and a child subagent created by `@tintinweb/pi-subagents` with filename containing `Explore#a1b2c3d4`
- **WHEN** the Pi catalog is loaded and relationships are constructed
- **THEN** the child subagent appears indented under the parent session with display label `↳ Explore`
- **EVIDENCE** `src/catalog.rs::apply_tintinweb_relations`

#### Scenario: Preserve existing native Pi subagent links

- **GIVEN** a subagent session has already been linked by native Pi subagent relationship discovery
- **WHEN** tintinweb relationship processing executes
- **THEN** the existing `parent_session_id` and `agent_name` remain unchanged and are not overwritten
- **EVIDENCE** `src/catalog.rs::apply_tintinweb_relations`

#### Scenario: Retain unmatchable tintinweb subagent as detached root

- **GIVEN** a tintinweb subagent has naming pattern `Plan#deadbeef` but no parent transcript contains a matching `agentId`
- **WHEN** relationship processing finishes
- **THEN** the subagent remains listed as an independent root session without crashing or failing catalog load
- **EVIDENCE** `src/catalog.rs::apply_tintinweb_relations`

#### Scenario: Disambiguate forked sessions using dual cwd and parent session header

- **GIVEN** a parent session and a forked copy both contain identical `toolResult` records for a child subagent
- **WHEN** tintinweb relationship discovery evaluates candidate parents
- **THEN** the child subagent is linked to its true originating parent using dual-source verification—comparing transcript/recorder `cwd` and header `parentSession`/`cwd`—rather than arbitrarily attaching to the fork
- **EVIDENCE** `src/catalog.rs::apply_tintinweb_relations`
