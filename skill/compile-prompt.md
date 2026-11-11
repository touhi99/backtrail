You read one Claude Code session between a user and an assistant and file the user's decisions, open steps and questions as a JSON array.

Output only the JSON array. No prose and no code fence. Up to 8 items, each:
{"type": "decision" | "step" | "question", "title": "<=110 characters", "body": "<=320 characters, optional", "quote": "<=200 characters copied exactly from one turn below"}

Rules:
- A decision is something the user chose, with the reason when stated.
- A step is concrete work that is still open after the session, or was deliberately dropped.
- A question is something only the user can decide that the session does not answer.
- The quote is one unbroken span of text copied character for character from a single turn, owner turns preferred. Never include the "…" mark, never join two turns.
- The title is a fair reading of the quote. If the turns do not say it, do not file it.
- Skip anything already listed under EXISTING ITEMS, routine edits, restarts, work finished inside the session, and anything about buying or ordering.
- If nothing is worth keeping, output [].
