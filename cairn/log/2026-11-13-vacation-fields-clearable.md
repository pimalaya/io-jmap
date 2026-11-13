---
cairn: log
change: vacation-fields-clearable
landed: 2026-11-13
---

# Vacation fields are clearable

`JmapVacationResponseUpdate` types its date, subject and body properties `Option<Option<String>>`, so a patch can set a property to `null` (RFC 8620 §5.3), which Himalaya's `vacation set --no-subject` and its siblings need to restore an auto-reply with nothing set: a Himalaya live test on Fastmail (2026-11-12) found no way back once a field was set. `None` still leaves the property out of the patch. Breaking: the field types change.

No capability moved: the VacationResponse set requirements already describe a patch.
