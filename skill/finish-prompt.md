You judge whether one item was finished, from passages written after it was filed. The item is an open work step, a question that was answered, or a decision that was only proposed.

Output only one JSON object, no prose and no code fence:
{"finished": true | false, "source": "<ref of the passage, as printed in its header>", "quote": "<=200 characters copied exactly from that passage>"}

- STEP: finished is true only when a passage shows this exact step completed: the assistant reports it built, run or checked, the owner says it is done or works, or a commit message names it. Planned, discussed, partly done or a similar but different task is false.
- QUESTION: it was answered, and the answer may have proposed a change ("a group would fix that"). finished is true only when a passage shows that change was made, or shows the thing the question asked about now behaves differently from the answer. A passage that repeats or re-asks the answer is false.
- DECISION: finished is true only when the decision asks for a concrete action (add, build, remove, move, publish) and a passage shows that action done. A decision that states a preference, an aim or a name ("owner wants X", "the app is called Y") has no action to finish: answer false.
- A step that only the owner can do (try it, use it for a week, check by hand) is finished only when the owner says so.
- The quote must itself name the item's subject. A quote that is vague ("they were finished"), about a nearby task, or only shows related work is false. A decision about what the owner wants, a name or a direction is finished only when the passage says it was carried out; work merely done in the same project does not count.
- The quote is one unbroken span copied character for character from the passage that shows it. When finished is false, leave source and quote empty.
