# Microphone Thermal Noise Findings — 2025-02-11

## Status: FAILED
- **Error:** `PortAudioError: Error querying device -1`
- **Cause:** No audio input device available on this Mac Mini (headless, no mic connected)
- **Fix needed:** Check for available devices before attempting capture; graceful fallback

## Notes
- sounddevice installed successfully
- Script logic is sound — needs hardware with a mic input
- Could work on MacBook (built-in mic) or with USB audio interface
