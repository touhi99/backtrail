You read one note or artifact from a work store, and what the store recorded on its thread since the doc was dated. Decide whether the doc is now stale. Output only one JSON object, no prose and no code fence:

{"id": "<DOC ref>", "stale": true or false, "reason": "<=300 characters", "by": ["<item id or doc ref from the packet>"]}

A doc is stale when any of these holds:
- A store item listed in the packet contradicts what the doc presents as current: a plan step the doc lists as to do is now done, rejected or parked; a choice the doc states was replaced by a later decision; a decision the doc relies on is superseded.
- A newer note or artifact listed in the packet covers the same subject and replaces this doc (a later version of the same plan, spec or page).
- The doc itself says it is superseded, replaced, archived or no longer current. Cite its own DOC ref in `by`.

Rules:
- `by` names the ids or refs from the packet that make the doc stale. Copy them exactly. Never invent one.
- One done step out of many open ones in a plan does not make the whole doc stale; stale means a reader acting on the doc today would be misled on its main point.
- A doc that records history (a log, a changelog, a session note about what happened) is not stale because later work followed it.
- A newer doc on a different subject in the same thread does not replace this one.
- When the packet does not show a clear contradiction or replacement, answer stale false with a short reason and an empty `by`. A false answer is correct when unsure.
- reason names the contradicting item or replacing doc in plain words, for example "Plan says to build the Mac menubar; d-22a2364c dropped the Mac app."
