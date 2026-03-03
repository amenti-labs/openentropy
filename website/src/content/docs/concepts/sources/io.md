---
title: 'IO Sources'
description: 'Storage and device path timing sources'
---

IO sources derive entropy from storage-stack, bus, and device timing effects.

## Sources

- `disk_io` — block I/O timing variability
- `fsync_journal` — journal flush path latency jitter
- `usb_enumeration` — USB device enumeration timing
- `nvme_iokit_sensors` — NVMe sensor/property polling timing
- `nvme_raw_device` — raw block-device read timing
- `nvme_passthrough_linux` — Linux NVMe admin passthrough timing
