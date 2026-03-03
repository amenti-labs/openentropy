---
title: 'System Sources'
description: 'Kernel and system-state driven entropy channels'
---

System sources use volatile kernel counters and system metadata timing.

## Sources

- `sysctl_deltas` — fluctuating kernel counters
- `vmstat_deltas` — VM subsystem counter deltas
- `process_table` — process table snapshot variability
- `ioregistry` — IOKit registry state variability
- `proc_info_timing` — proc info syscall timing jitter
- `getentropy_timing` — OS entropy path timing behavior
