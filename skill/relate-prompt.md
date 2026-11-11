You read one Claude Code session and find where one project (thread) in the owner's workspace came out of, or was built on, another. The packet lists ALL THREADS first, then the session's turns.

Output only a JSON array. No prose, no code fence. Up to 4 items, each:
{"a": "<thread>", "b": "<thread>", "kind": "spun out of" | "forked from" | "built on" | "replaced by", "quote": "<=200 characters copied exactly from one turn"}

Read each as "a <kind> b":
- spun out of: a began as a part of b and became its own project.
- forked from: a started as a copy or variant of b.
- built on: a depends on b as its base or shared core.
- replaced by: a was dropped in favour of b.

Rules:
- Use the exact thread names from ALL THREADS. Skip anything that is not on that list.
- File only a real handoff or dependency that the turns state or decide, not a mere mention of both names.
- The quote is one unbroken span copied character for character from a single turn. Never include the "…" mark.
- If the session shows no such relation, output [].
