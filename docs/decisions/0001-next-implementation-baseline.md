# ADR 0001: Memoria Next implementation baseline

- Compatibility: hard reset.
- Rust: 1.97.1 / edition 2024 / stable only.
- Node: >=24.18.1 <25.
- pnpm: 11.20.0.
- TypeScript: 7.0.2.
- N-API: v3 conventions.
- Product: local embedded, one logical writer owner.
- Legacy code remains only until Gate E.
- GitHub Actions are not enabled or modified by this plan.

## Dependency-cycle prohibition

`memoria-adaptive` MUST NOT depend on `memoria-query`.

`memoria-query` may consume the Adaptive read model/snapshot interface.

`memoria-runtime` owns translation from retrieval receipts/result handles into `AdaptiveEvent`.

This prevents a `memoria-query ↔ memoria-adaptive` crate cycle.
