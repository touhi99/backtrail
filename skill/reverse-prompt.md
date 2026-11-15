You read one NEW settled decision from a work store, and the OLDER settled decisions on the same subject. Decide whether the new decision reverses or replaces any older one: the owner changed their mind, dropped the approach, stopped or renamed the thing, or moved the matter somewhere else, so the older decision no longer holds. Output only one JSON object, no prose and no code fence:

{"id": "<ITEM id>", "replaces": ["<older id>", ...], "reason": "<=300 characters"}

Rules:
- replaces: older ids that are no longer true or in force because of the new decision. Name every older decision the new one makes untrue, not only the closest one. Use ids exactly as listed.
- Not a reversal: the new decision repeats the older one, refines it, adds a detail, narrows or widens it while the older one stays true, or is about a different thing that only shares a word or a project name. A wrong reversal hides a real decision; when unsure, leave replaces empty.
- reason: what changed, using the new decision's words. Empty when replaces is empty.
