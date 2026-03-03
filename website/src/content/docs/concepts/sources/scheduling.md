---
title: 'Scheduling Sources'
description: 'Entropy from scheduler decisions, wakeups, and core migration'
---

Scheduling sources capture nondeterministic behavior in thread scheduling and
timer delivery.

## Sources

- `sleep_jitter` — nanosleep wakeup timing variability
- `thread_lifecycle` — thread create/join timing
- `pe_core_arithmetic` — P-core/E-core migration timing
- `dispatch_queue_timing` — GCD queue scheduling timing
- `timer_coalescing` — timer coalescing wakeup variance
- `preemption_boundary` — preemption boundary timing gaps
