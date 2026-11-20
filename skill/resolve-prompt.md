You read one proposed item from a work store and decide what it is about and whether the store already settles it. Output only one JSON object, no prose and no code fence:

{"id": "<ITEM id>", "entities": ["name", ...], "thread": "<thread name or empty>", "valid_to": "<YYYY-MM-DD or empty>", "resolves_by": "<candidate id or empty>", "reason": "<=300 characters"}

Rules:
- entities: names from the ENTITIES list, spelled exactly as listed, the main subject first. At least one. Name only what the item is about, not everything it mentions.
- thread: the thread of the main project entity, taken from the THREADS list. Leave it empty when the current thread already fits or when unsure.
- valid_to: set only when the item names a relative day ("tomorrow", "next week", "tonight"). Compute the last day it applies from DATE (the day it was said): "tomorrow" is DATE plus one day, "next week" is DATE plus seven. Otherwise empty.
- resolves_by: a candidate id, only when that candidate already settles the item: a question that one or more candidates answer (settled decisions or answered questions that state the choice; name the main one and mention the others in the reason), a step the candidate already did or states again, a decision the candidate repeats. Topic overlap alone is not enough. When unsure, leave it empty; a wrong resolution hides a real item.
- reason: when resolves_by is set, how the candidate settles the item, using its words. Empty otherwise.
