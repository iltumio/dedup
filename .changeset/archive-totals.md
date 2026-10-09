---
default: patch
---

# Improve archive migration finalization

Count each referenced chunk once and scan blob directories in filesystem order
to speed up final storage totals. Show actual finalization progress and allow
cancellation and resumption without losing verified files.
