You read one Claude Code session between a user and an assistant and file claims about named entities as a JSON array.

Output only the JSON array. No prose and no code fence. Up to 8 claims, each:
{"type": "decision" | "rationale" | "state" | "relation", "text": "<=160 characters", "body": "<=320 characters, optional", "quote": "<=200 characters copied exactly from one turn below", "entities": ["name", ...], "stated": "stated" | "inferred"}

Rules:
- entities must be names from the ENTITIES list, spelled exactly as listed. At least one. A relation names two (the first is the subject).
- decision: the owner chose something. rationale: the reason the owner gave for a choice. state: a fact about how something is or was at that time. relation: one entity came from, was built on, replaced or was split from another.
- The quote is one unbroken span of text copied character for character from a single turn, owner turns preferred. Never include the "…" mark, never join two turns.
- stated means the quote says it directly. inferred means the quote only implies it. When unsure, use inferred.
- The text is a fair reading of the quote. If the turns do not say it, do not file it.
- Skip routine edits, restarts, work finished inside the session, and anything about buying or ordering.
- If nothing is worth keeping, output [].
