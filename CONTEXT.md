# spawn-agent

A Linux CLI that spawns AI-agent harnesses into new tmux panes as teammates of the agent that spawned them, and wires up the communication between the two.

## Language

**Harness**:
An agentic coding CLI (Claude Code, Codex) that runs an AI agent interactively in a terminal.
_Avoid_: agent CLI, tool (overloaded)

**Spawn**:
The act (and command) of creating a teammate. Whoever runs it becomes the new teammate's lead.
_Avoid_: create, launch

**Teammate**:
A harness instance created by spawn, working under a lead. Addressed by a unique name derived from a user-chosen label; uniqueness is guaranteed by the tool, never by the user.
_Avoid_: spawn (as a noun), worker, sub-agent

**Lead**:
The agent (or human) that spawned a given teammate. Always relative to a teammate — there is no global lead.
_Avoid_: leader, orchestrator, master

**Adapter**:
The per-harness glue that maps spawn-agent's protocol — turn-end reporting, preamble injection — onto that harness's native extension points.
_Avoid_: wrapper (implies supervising the process; adapters don't)

**Message**:
Any communication delivered to an inbox — instructions, questions, updates, reports. Questions are not a separate kind of message.

**Turn-end report**:
The message a teammate's adapter automatically sends its lead when a turn finishes — whoever triggered the turn — carrying the turn's final assistant text.
_Avoid_: result, notification

**Inbox**:
Where a recipient's delivered messages wait; a message is unread until the recipient consumes it.
_Avoid_: queue, mailbox, spool

**Nudge**:
The short keystroke tap typed into a recipient's pane to say "check your inbox". Carries no payload.
_Avoid_: ping, wake

**Retrier**:
The short-lived process left behind whenever a message is delivered, repeating the nudge until the message is consumed or the recipient is gone. One per message.
_Avoid_: daemon, watcher

**Preamble**:
The starting instructions a teammate's adapter injects — who it is, who its lead is, how to communicate — as opposed to the task instructions the caller supplies.
