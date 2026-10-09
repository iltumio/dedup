# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com), this project
adheres to [Semantic Versioning](https://semver.org), and the changelog is maintained
automatically by [Knope](https://knope.tech) from
[Conventional Commits](https://www.conventionalcommits.org).

## 0.2.2 (2026-10-09)

### Features

- add FastCDC archives and resumable parallel migration

## 0.2.1 (2026-10-09)

### Features

- add FastCDC archives and resumable parallel migration

#### FastCDC archives and resumable migration

Add FastCDC chunk deduplication with LZ4 compression to share unchanged content between similar files while preserving whole-file content identifiers. Existing original archives remain readable and writable, and the desktop app now lets users choose FastCDC or the original whole-file format when creating an archive.

Migrate original archives from the desktop app or the CLI into a separate destination with verified, resumable checkpoints. Migration supports configurable parallel file processing, including changing the worker count when resuming, and works on exFAT drives and in dedicated subfolders when the original archive occupies a disk root.

Show elapsed time, processed bytes, read and verification rate, active files and per-file progress during migration. Periodic updates distinguish app responsiveness from stalled data progress. Count shared chunks once in archive and extension statistics.

## 0.2.0 (2026-10-03)

### Features

- new vector app icon aligned with in-app brand
- consolidate add-folder calls to action

#### New app icon

Replaced the app icon with a vector "many copies → one file" mark that stays legible at small sizes and matches the logo in the sidebar. Linux packages now install every hicolor size plus a scalable SVG, and the desktop entry sets `StartupWMClass` so the window is matched to its launcher icon.

### Fixes

#### One clear way to add a folder

The header button now adds into the folder you are viewing ("Add to Vacations") and replaces the separate "Add here" link. The scan form shows the full destination with a Change link, the header button steps back when an empty folder offers its own call to action, and duplicate "Add folder" buttons were removed from the Activity view and the no-archive screen.

## 0.1.7 (2026-09-13)

### Features

- modernize desktop UX and fix file opening

#### Modernize the desktop app and improve responsiveness

- Add a dedicated scan progress screen with a stop action and persistent session results.
- Simplify archive navigation with separate files, duplicates, activity, and overview views, folder search, sorting, and paginated lists.
- Add consistent light, dark, and system themes and accessible Svelte select menus.
- Improve archive setup, reversible removal from the archive list, file details, and error feedback.
- Move blocking archive operations off the UI thread and optimize directory metadata queries.
- Fix Linux file opening for terminal-based editors, including log files, and hide Open file when no default application is associated with a recognized file type.
- Add regression tests for responsiveness, file launching, navigation, and accessibility.

## 0.1.6 (2026-07-11)

### Features

- add scan options API
- bundle git directories during scans
- expose git bundling scan option
- add git bundling scan checkbox
- add scan rule types
- ignore paths with scan rules
- archive directories with scan rules
- expose scan rules through commands
- persist custom scan rules
- add scan rule controls
- add shared UI primitives
- add compact app shell
- extract workspace UI
- extract scan workflow UI
- parallel scan pipeline with batched metadata commits

### Fixes

- harden git directory bundling
- preserve git bundling scan semantics
- default git bundling tauri option
- stabilize scan rules
- preserve legacy CSS tokens during Tailwind setup
- harden shared UI primitives
- stabilize app shell layout
- keep dialog actions visible
- size main page content
- wrap duplicate paths
- polish responsive UI redesign
- address UI review polish

#### Fix Arch Linux package build in the release pipeline

The Arch build job now installs pnpm via npm instead of corepack, which is no
longer bundled with Arch's `nodejs` package. This unblocks the release workflow
so Arch packages are built and published again.

## 0.1.5 (2026-07-11)

### Features

- add scan options API
- bundle git directories during scans
- expose git bundling scan option
- add git bundling scan checkbox
- add scan rule types
- ignore paths with scan rules
- archive directories with scan rules
- expose scan rules through commands
- persist custom scan rules
- add scan rule controls
- add shared UI primitives
- add compact app shell
- extract workspace UI
- extract scan workflow UI
- parallel scan pipeline with batched metadata commits

### Fixes

- harden git directory bundling
- preserve git bundling scan semantics
- default git bundling tauri option
- stabilize scan rules
- preserve legacy CSS tokens during Tailwind setup
- harden shared UI primitives
- stabilize app shell layout
- keep dialog actions visible
- size main page content
- wrap duplicate paths
- polish responsive UI redesign
- address UI review polish

## 0.1.4 (2026-03-26)

Baseline release. Automated changelog and release management begins with the next version.
