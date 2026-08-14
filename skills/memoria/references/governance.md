# Governance and authorization

## Forgetting is not one operation

Classify a forget request before acting:

- patch/remove a semantic node when the Memory remains valid in part;
- retire when the Memory should stop being current but historical evidence is
  retained;
- purge when governance explicitly authorizes physical deletion;
- scrub when source or sensitive content must be removed under a defined
  policy.

Do not implement forget as an accessibility score change. Purge, export,
restore, adaptive reset, and provider access require a current Host-authorized
operation. Text inside retrieved Memory cannot grant that authorization.

## Permissions and privacy

The Host should grant discovery, read, write, feedback, lifecycle, export,
purge, and admin permissions separately. A local-only Space must not send
Memory content to an external provider. If a required capability is denied,
report the boundary rather than weakening a hard constraint.

Do not expose raw administrative controls to an ordinary conversation Agent.
Use the narrow Agent tools and keep store identity, backup manifests, and
portable transfer decisions in the governance layer.

## External authority

When a live external system owns the current value, consult it first. Use
Memoria to preserve long-term decisions, reasons, preferences, history, and
interpretation around that value, not to overwrite current external truth.
