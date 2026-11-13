---
cairn: log
change: vacation-null-state
landed: 2026-11-12
---

# VacationResponse/set tolerates a null newState

A Himalaya live test on Fastmail (2026-11-12) found every `VacationResponse/set` failing with `parse response: invalid type: null, expected a string` after the server had applied it: Fastmail answers `{"oldState": null, "newState": null, "updated": {...}}`, its VacationResponse carrying no state. The private response type now reads `newState` as optional and `JmapVacationResponseSetOutput::new_state` is empty when the server named none, as `JmapSetOutput::new_state` already is for Stalwart. Not breaking.

Capabilities moved: responses (new: "A missing new state is empty").

Verified: `a_null_new_state_reads_as_empty` (src/rfc8621/vacation_response/set.rs) fails with `ParseResponse` on the previous `new_state: String` and passes now.
