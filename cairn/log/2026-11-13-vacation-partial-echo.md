---
cairn: log
change: vacation-partial-echo
landed: 2026-11-13
---

# VacationResponse/set reads a partial echo

Re-verifying the 2026-11-12 vacation-null-state fix live on Fastmail showed `VacationResponse/set` still failing after the update was applied, now with `parse response: missing field id`: Fastmail echoes `"updated": {"singleton": {"fromDate": null}}`, the properties it was sent, which RFC 8620 §5.3 allows as a partial object. The private response type now reads each echoed value as JSON and `JmapVacationResponseSetOutput::updated` is filled only when it is a whole VacationResponse. Not breaking.

No capability moved: responses already says a set echoes what changed.
