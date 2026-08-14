# Explicit feedback lifecycle

Adaptive learning may come only from explicit Host feedback after evidence was
actually used or judged. Query exposure, top-k selection, and mere return do
not constitute feedback.

Returned-but-unused is not negative feedback. If the Agent returned evidence
but the user did not use or judge it, submit no event.

After actual use, submit_memory_feedback with the stable retrieval ID, result
ID, pinned revision, idempotency key, and the smallest honest outcome. Use a
positive outcome only when the evidence helped or was preferred. Use a
negative outcome only when the user or Host actually rejected, corrected, or
judged it as incorrect. Keep Authority correction separate from retrieval
feedback: correcting a Memory does not by itself describe whether the
retrieval was useful.

Feedback is a persistent write capability. Respect Host permissions, scope,
revision pinning, and idempotency. Never fabricate a retrieval ID, result ID,
Memory ID, Revision ID, or semantic node ID.
