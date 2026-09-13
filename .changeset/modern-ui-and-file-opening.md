---
default: minor
---

# Modernize the desktop app and improve responsiveness

- Add a dedicated scan progress screen with a stop action and persistent session results.
- Simplify archive navigation with separate files, duplicates, activity, and overview views, folder search, sorting, and paginated lists.
- Add consistent light, dark, and system themes and accessible Svelte select menus.
- Improve archive setup, reversible removal from the archive list, file details, and error feedback.
- Move blocking archive operations off the UI thread and optimize directory metadata queries.
- Fix Linux file opening for terminal-based editors, including log files, and hide Open file when no default application is associated with a recognized file type.
- Add regression tests for responsiveness, file launching, navigation, and accessibility.
