# Sync Report

## session-catalog
- ADDED Discover sessions for the active engine
- ADDED Keep engine catalogs separate
- ADDED Present safe session summaries and metadata
- ADDED Degrade per file without discarding healthy sessions
- ADDED Reject sessions outside the engine session area

## session-deletion
- ADDED Require explicit deletion confirmation
- ADDED Restrict deletion to the active engine's session area
- ADDED Update the catalog after a single deletion
- ADDED Support confirmed group deletion with partial-failure reporting
- ADDED Clear pending deletion when engine context changes

## session-detail
- ADDED Show the selected session's detail
- ADDED Preserve user and assistant content faithfully
- ADDED Apply transcript display configuration with safe defaults
- ADDED Degrade locally on malformed transcript lines
- ADDED Page large transcripts
- ADDED Reject detail reads outside the engine session area

## session-launch
- ADDED Resume the selected session with its engine
- ADDED Start a new engine session in a selected project
- ADDED Validate the working directory before launch
- ADDED Report external command failures and preserve context

## terminal-lifecycle
- ADDED Activate an interactive terminal session safely
- ADDED Roll back partial terminal activation
- ADDED Restore the user's terminal on exit
- ADDED Suspend and rebuild the TUI around external engine commands
- ADDED Exit without touching an absent terminal after quit

## tui-interaction
- ADDED Navigate engines and session trees
- ADDED Support project and time grouping
- ADDED Operate list and detail panels by keyboard and mouse
- ADDED Adjust layout within usable bounds
- ADDED Provide modal interaction safeguards
- ADDED Quit explicitly
