---
default: minor
---

# New app icon

Replaced the app icon with a vector "many copies → one file" mark that stays legible at small sizes and matches the logo in the sidebar. Linux packages now install every hicolor size plus a scalable SVG, and the desktop entry sets `StartupWMClass` so the window is matched to its launcher icon.
