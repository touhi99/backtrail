You read one NEW settled decision from a work store, the OLDER settled decisions on the same subject, and REASONS said in the same session. Name how the new decision links to them. Output only one JSON object, no prose and no code fence:

{"id": "<ITEM id>", "replaces": ["<older id>", ...], "restates": ["<older id>", ...], "because": ["<reason id>", ...], "reason": "<=300 characters"}

Rules:
- replaces: older decisions that are no longer true or in force because of the new one: the owner changed their mind, dropped the approach, stopped or renamed the thing, or moved the matter somewhere else. Name every older decision the new one makes untrue. Not a reversal: a repeat, a refinement, an added detail, a narrower or wider version that leaves the older one true, or a different thing that only shares a word or project name.
- restates: older decisions that say the same thing as the new one in other words, so both stay true. Not a restatement: a refinement that adds a new choice, or a decision about a different thing.
- because: reasons from the REASONS list that explain why the new decision was made. A reason that explains a different decision from the same session does not count.
- An older id goes in at most one of replaces and restates. When unsure, leave it out; a wrong link misleads more than a missing one.
- reason: what changed, using the new decision's words, when replaces is not empty. Empty otherwise.
