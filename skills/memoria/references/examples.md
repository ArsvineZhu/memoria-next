# Worked workflows

## Historical recall

Resolve the speaker to the Host EntityRef, choose the personal Space, and ask
for a historical State around the explicit time range. Return the pinned
revision and its source. Do not answer from a current State when the question
asks what was true earlier.

## New fact with an existing owner

Discover the education-career owner using the Space key, resolved user
EntityRef, and an explicit cue. Read the current revision. Choose ADD or UPDATE
and submit a typed patch with expectedHead. Do not create a second career
Memory merely because the wording differs.

## State transition

When a plan changes, close the previous State with its validTo and add the new
State with validFrom. Keep the old State queryable in history. This is
TRANSITION, not a prose overwrite.

## Correction

If a year or entity was represented incorrectly, read the current revision,
choose CORRECT, and preserve the correction provenance. If the incorrect
Memory affected an answer, submit separate negative feedback only after that
retrieval was actually judged.

## Ambiguous alias

If scoped observations return two people for the same alias, return
ambiguous candidates and ask for context or confirmation. Do not choose the
highest-scoring candidate. Memoria ranking is not identity truth.

## HEAD conflict

Read the latest Memory after a conflict, compare the new content with the
current intent, then regenerate, no-op, or ask. Do not blindly retry HEAD
conflicts.
