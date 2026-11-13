---
cairn: spec
capability: requests
status: current
---

# Requests

What io-jmap refuses or escapes in the values a caller hands it before they reach the wire, in a request head or in a method call.

### Requirement: A blob content type stays in its header
`JmapBlobUpload::new` SHALL refuse a `content_type` holding a CR, LF or NUL with `InvalidContentType`, before any request is built, these bytes being invalid in a field value (RFC 9110 §5.5). `JmapClientStd::blob_upload` SHALL surface it as `BlobUpload(InvalidContentType)`.

#### Scenario: A content type carrying a header line
- GIVEN the content type `text/plain\r\nX-Injected: 1`
- WHEN `JmapBlobUpload::new` is called
- THEN it returns `InvalidContentType` and no request is written

### Requirement: Email patch paths are JSON Pointers
`JmapEmailPatch` SHALL write the keyword of `SetKeyword` and `UnsetKeyword` and the mailbox id of `AddToMailbox` and `RemoveFromMailbox` as one JSON Pointer token (RFC 6901 §3), `~` as `~0` and `/` as `~1`, the way `JmapCalendarEventPatch::diff` writes a recurrence id, RFC 8621 §4.1.1 allowing both characters in a keyword.

#### Scenario: A keyword holding a slash
- GIVEN the patch `set_keyword("todo/later")`
- WHEN it is serialised
- THEN its path is `keywords/todo~1later`
