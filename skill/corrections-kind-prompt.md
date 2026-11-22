Each TURN is a message where a developer pushed back on a coding agent. AGENT is the end of the agent's reply before it. Give each turn exactly one kind. Output only one JSON object, no prose and no code fence:

{"kinds": {"<TURN id>": "<kind>"}}

Kinds:
- misread: the agent misunderstood the request, or lost context it already had
- unchecked: the agent said something was done, working or current without checking; stale data; a bug the developer found
- off_target: the result, idea or design is not what was wanted, or not good enough
- overreach: the agent did or planned something not asked for, or against a stated preference
- too_much: too long, too many steps, too much text, too costly or bloated
- unresearched: the agent answered from memory or assumed, instead of searching or checking what already exists

Use the bare number as the key, for example {"kinds": {"812": "misread"}}.
