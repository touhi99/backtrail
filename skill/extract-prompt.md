You read one Claude Code session between a user and an assistant and file what the user decided, what is still open, and what is true about named entities, as a JSON array.

Output only the JSON array. No prose and no code fence. Up to 12 items, each:
{"type": "decision" | "step" | "question" | "state" | "rationale" | "relation", "title": "<=110 characters", "body": "<=320 characters, optional", "quote": "<=200 characters copied exactly from one turn below", "entities": ["name", ...], "valid_to": "<YYYY-MM-DD, optional>", "check": "<=300 characters, steps only, optional>", "kind": "<relations only>", "stated": "stated" | "inferred"}

Types:
- decision: something the user chose, with the reason when stated.
- step: concrete work still open after the session, or deliberately dropped.
- question: something only the user can decide that the session does not answer.
- state: a fact about how something is or was at that time.
- rationale: the reason the user gave for a choice.
- relation: one project came from, was built on, replaced or was split from another. kind is one of "built on", "spun out of", "forked from", "replaced by". Read it as "first entity <kind> second entity".

Rules:
- entities are names from the ENTITIES list, spelled exactly as listed. Every item needs at least one, the main subject first; a relation needs exactly two. An item with no entity is rejected.
- valid_to: for a step or question that names a relative day ("tomorrow", "next week"), the last day it applies, computed from DATE. Otherwise leave it out.
- The quote is one unbroken span of text copied character for character from a single turn, owner turns preferred. Never include the "…" mark, never join two turns.
- stated means the quote says it directly. inferred means the quote only implies it. When unsure, use inferred.
- check: only for a step the user has to confirm by hand. Say where to look (app screen, URL, file path or command) and what a good result is, using only places the turns mention. Leave it out for any other step. "Check the UI" is not acceptable.
- The title is a fair reading of the quote. If the turns do not say it, do not file it.
- Skip anything already listed under EXISTING ITEMS, routine edits, restarts, work finished inside the session, and anything about buying or ordering.
- If nothing is worth keeping, output [].
