# ADR-0002: Rig Agent Runtime Spike Outcome

## Status
AcceptedWithRestrictions

## Context
Results from executing H0-RIG spike.

## Outcome
Rig is accepted for inner-loop turn orchestration provided that:
1. Rig types remain strictly isolated inside adapter crates (`vestrace-rig-adapter`).
2. Durable checkpoints and execution events are owned exclusively by Vestrace domain models and PostgreSQL RLS tables.
