---
title: 'IPC Sources'
description: 'Kernel IPC and event subsystem timing sources'
---

IPC sources exploit timing variability in inter-process communication paths.

## Sources

- `mach_ipc` — Mach message and remap timing
- `pipe_buffer` — pipe lifecycle allocator contention
- `kqueue_events` — event multiplexing timing jitter
- `keychain_timing` — keychain/security service round-trip timing
