You read one coding-agent session between a user and an assistant and file what the user decided, what is still open, and what is true about named entities, as a JSON array.

Output only the JSON array. No prose and no code fence. Up to 12 items, each:
{"type": "decision" | "step" | "question" | "state" | "rationale" | "relation" | "alternative" | "depends" | "finished", "title": "<=110 characters", "body": "<=320 characters, optional", "quote": "<=200 characters copied exactly from one turn below", "entities": ["name", ...], "valid_to": "<YYYY-MM-DD, optional>", "check": "<=300 characters, steps only, optional>", "kind": "<relations only>", "of": "<decision title, alternatives only>", "step": "<step title, depends only>", "on": "<step title, depends only>", "stated": "stated" | "inferred"}

Types:
- decision: something the user chose, with the reason when stated.
- step: concrete work still open after the session, or deliberately dropped. A failure the assistant worked around without telling the owner (a failed tool call, then a different approach) is a step: fix the cause. Not for a failure the owner saw, or a transient error retried the same way.
- question: something only the user can decide that the session does not answer.
- state: a fact about how something is or was at that time.
- rationale: the reason the user gave for a choice.
- relation: one project came from, was built on, replaced or was split from another. kind is one of "built on", "spun out of", "forked from", "replaced by". Read it as "first entity <kind> second entity".
- alternative: an option the owner considered and rejected. `of` is the exact title of a decision item in this output or an existing decision. Shape: {"type": "alternative", "of": "<decision title>", "title": "<=110 characters", "body": "<=320 characters, why it was rejected", "quote": "<=200 characters", "entities": ["name", ...]}.
- depends: one step cannot proceed until another is done, as the turns state it. `step` is the exact title of a step item in this output or an existing step; `on` is the step it waits for. Shape: {"type": "depends", "step": "<step title>", "on": "<step title>", "quote": "<=200 characters"}.
- finished: a step from OPEN STEPS that these turns show finished: the assistant reports it built, run or checked, or the owner says it is done or works. Shape: {"type": "finished", "step": "<id from OPEN STEPS>", "quote": "<the line that shows it>"}. Not for a step that was only discussed, planned or partly done.

Rules:
- entities are names from the ENTITIES list, spelled exactly as listed. Every item needs at least one, the main subject first; a relation needs exactly two. An item with no entity is rejected. A finished item needs none.
- valid_to: for a step or question that names a relative day ("tomorrow", "next week"), the last day it applies, computed from DATE. Otherwise leave it out.
- The quote is one unbroken span of text copied character for character from a single turn, owner turns preferred. A workaround step quotes the failed tool turn. Never include the "…" mark, never join two turns.
- TURNS can include `tool` turns: a failed tool call with its error. A tool turn is not the owner speaking.
- stated means the quote says it directly. inferred means the quote only implies it. When unsure, use inferred.
- check: only for a step the user has to confirm by hand. Say where to look (app screen, URL, file path or command) and what a good result is, using only places the turns mention. Leave it out for any other step. "Check the UI" is not acceptable.
- The title is a fair reading of the quote. If the turns do not say it, do not file it.
- alternative: file only when the turns say an option was considered and rejected, not when someone merely could choose it. `of` must match a decision title exactly; if none, skip the alternative.
- depends: file only when the turns say one step waits for or is blocked by another. Do not infer order from the list. `step` and `on` must match step titles exactly; if either is missing, skip the dependency.
- Skip anything already listed under EXISTING ITEMS, routine edits, restarts, other work finished inside the session (file a finished item only for a step in OPEN STEPS), and anything about buying or ordering.
- If nothing is worth keeping, output [].
