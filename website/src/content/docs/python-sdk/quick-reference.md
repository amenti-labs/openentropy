---
title: 'Python Quick Reference'
description: 'Most-used Python SDK calls for day-to-day workflows'
---

Use this page for the common 80% workflows. For full signatures and all
functions, see [Python API Reference](/openentropy/python-sdk/reference/).

## Pool Basics

```python
from openentropy import EntropyPool

pool = EntropyPool.auto()
pool.collect_all(parallel=True, timeout=5)
data = pool.get_random_bytes(256)
raw = pool.get_raw_bytes(256)
health = pool.health_report()
```

## Source Discovery

```python
from openentropy import detect_available_sources, platform_info
sources = detect_available_sources()
platform = platform_info()
```

## Dispatcher Analysis

```python
from openentropy import analyze
report = analyze([("src", data)], profile="deep")
```

## Focused Analysis Calls

```python
from openentropy import full_analysis, chaos_analysis, trial_analysis
forensic = full_analysis("src", data)
chaos = chaos_analysis(data)
trials = trial_analysis(data)
```

## Session Workflows

```python
from openentropy import record, list_sessions, load_session_raw_data
meta = record(pool, ["clock_jitter"], duration_secs=30.0)
sessions = list_sessions("sessions")
raw_map = load_session_raw_data(sessions[0]["path"])
```

## Related

- [Python API Reference](/openentropy/python-sdk/reference/)
- [Analysis System](/openentropy/concepts/analysis/)
