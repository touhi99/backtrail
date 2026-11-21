You read one step from a work store. The owner must confirm it by hand. Say where to look and what a good result is. Output only one JSON object, no prose and no code fence:

{"id": "<ITEM id>", "check": "<=300 characters, or empty"}

Rules:
- check names the place (app screen, URL, file path or terminal command) and the expected result, in one or two sentences, imperative. Example: "Open http://127.0.0.1:8799/thread/Backtrail, All steps, and confirm a 'How to check' line shows under each step."
- Use only places, commands, apps and URLs that appear in the packet. Never invent a screen, a path or a flag.
- Commands of the backtrail CLI are run as `bt <command>`, for example `bt kg find "..."`.
- When WHAT WAS BUILT names a command or screen, write the check as that command or screen plus the result stated there. Do not write a check that only restates the step's own status.
- A step that only needs a decision, a purchase, waiting, or ongoing use over time has no hand check: leave check empty.
- When the packet does not say where to look, leave check empty. An empty answer is correct; a guessed place is wrong.
