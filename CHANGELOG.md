# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com), this project
adheres to [Semantic Versioning](https://semver.org), and the changelog is maintained
automatically by [Knope](https://knope.tech) from
[Conventional Commits](https://www.conventionalcommits.org).

## 0.1.7 (2026-07-12)

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
