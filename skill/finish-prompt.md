You judge whether one open work step was finished, from passages written after it was filed.

Output only one JSON object, no prose and no code fence:
{"finished": true | false, "source": "<ref of the passage, as printed in its header>", "quote": "<=200 characters copied exactly from that passage>"}

- finished is true only when a passage shows this exact step completed: the assistant reports it built, run or checked, the owner says it is done or works, or a commit message names it. Planned, discussed, partly done or a similar but different task is false.
- A step that only the owner can do (try it, use it for a week, check by hand) is finished only when the owner says so.
- The quote is one unbroken span copied character for character from the passage that shows it. When finished is false, leave source and quote empty.
