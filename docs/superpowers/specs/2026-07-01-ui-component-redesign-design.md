# UI Component Redesign Design

Date: 2026-07-01

## Context

The app is a SvelteKit/Tauri frontend for managing dedup workspaces, scanning directories, browsing stored files, inspecting file details, and reviewing deduplication stats. The current frontend uses local Svelte component styles, a small global CSS token file, and a large `app/src/routes/+page.svelte` that owns most shell, dialog, workspace, scan, and scan-rule UI.

The redesign covers the whole app UI. It should convert the frontend to Tailwind CSS v4 and DaisyUI v5, extract reusable Svelte components, and reorganize the app into a compact operations console. The selected visual direction is DaisyUI's Night theme base. The selected UX scope is a broader pass across navigation, empty states, stats placement, scan/workspace dialogs, and file/details layout.

Official setup references:

- Tailwind SvelteKit install: `https://tailwindcss.com/docs/installation/framework-guides/sveltekit`
- DaisyUI SvelteKit install: `https://daisyui.com/docs/install/sveltekit/`

## Goals

- Build a reusable UI component layer over DaisyUI and Tailwind utilities.
- Reduce `+page.svelte` to orchestration: app state, API calls, event handlers, and component composition.
- Keep the app dense, quiet, and operations-focused.
- Preserve the existing feature set and data contracts.
- Improve hierarchy and flow for workspaces, scanning, custom scan rules, file browsing, details, stats, progress, cancellation, errors, and empty states.
- Keep scan-rule semantics unchanged: built-in presets first, then selected custom rules in saved order.

## Non-Goals

- No backend scan behavior changes.
- No new scan-rule actions.
- No routing overhaul unless it is required by SvelteKit/Tailwind setup.
- No marketing landing page or decorative hero UI.
- No theme switcher in this pass.

## External UI Stack

Install Tailwind CSS v4 and DaisyUI v5 in `app`.

Expected setup:

- Add `tailwindcss`, `@tailwindcss/vite`, and `daisyui` to the app dependencies/dev dependencies according to the package manager layout.
- Update `app/vite.config.ts` to import `tailwindcss` from `@tailwindcss/vite` and include `tailwindcss()` before `sveltekit()` in `plugins`.
- Replace the current global CSS token approach in `app/src/app.css` with Tailwind and DaisyUI imports:

```css
@import "tailwindcss";
@plugin "daisyui";
```

- Configure DaisyUI to use the Night theme base. Add only the app-specific global CSS required for shell height, body overflow, font smoothing, stable scroll containers, and monospace path text.

## App Shell

The app becomes a compact operations console.

The top bar owns global state and global actions:

- App identity.
- Workspace switcher.
- Files/Stats segmented navigation.
- Aggregate stat chips for files, duplicates, and saved bytes.
- Primary Scan action.

The Files view uses a stable two-area layout:

- Left: fixed-width file browser column with overflow contained inside the tree.
- Right: flexible details workspace with a compact stats summary band and a selected-file inspector.

The Stats view remains accessible from the top bar and uses the same shell, spacing, and stat components. It should feel like an operational report, not a separate visual language.

Responsive behavior:

- Desktop keeps the two-column file/details layout.
- Narrow widths collapse the detail area below or behind the file browser only if necessary for readability.
- Text must not overlap controls. Long paths and regex patterns must truncate or wrap intentionally.

## Component Architecture

Create focused Svelte components under `app/src/lib/components`.

Shared primitives:

- `UiButton`: DaisyUI button wrapper for primary, secondary, ghost, destructive, icon, and loading states.
- `UiDialog`: accessible modal frame with title, description slot, body slot, and action slot.
- `UiField`: label, hint, error, and input slot.
- `UiSegmentedControl`: compact tabs for Files/Stats and similar binary controls.
- `UiStat`: dense stat chip/card for files, duplicates, saved bytes, scan count, and progress metrics.
- `UiBadge`: status/action badge for tags, ignore/archive actions, and workspace labels.
- `UiEmptyState`: consistent empty states for no workspace, no selected file, no stats, and no custom rules.

App/domain components:

- `AppShell`: top bar, workspace control slot, navigation, aggregate stats, primary scan action, and content slot.
- `WorkspaceManagerDialog`: dialog shell and state-specific body for list/create/import flows.
- `WorkspaceForm`: create/import workspace form.
- `WorkspaceListItem`: dense workspace row with label, path, tags, scan count, files, duplicates, and saved bytes.
- `ScanDialog`: scan workflow shell and orchestration props.
- `ScanPresetList`: built-in preset toggles.
- `CustomScanRuleList`: saved rule rows with per-scan enablement.
- `CustomScanRuleEditor`: inline add-rule form with label, regex, and ignore/archive action.
- `ScanProgressPanel`: progress metrics, current path, and cancel action.

Existing domain components stay, but are restyled and refined:

- `FileTree`
- `TreeNode`
- `FileDetails`
- `StatsPage`

`app/src/routes/+page.svelte` should keep state and API coordination, then compose the extracted components. It should not retain large blocks of dialog markup or most CSS.

## Scan Flow

The scan dialog is a compact workflow with clear sections.

Location:

- Source directory input.
- Virtual target path input.
- Path inputs use monospace styling.

Presets:

- `.git archive`
- Rust `target` ignore
- Node `node_modules` ignore
- Python `.venv` and `venv` ignore

All presets remain opt-in per scan.

Saved custom rules:

- App-level saved rules appear as reusable rows.
- Rows show label, action badge (`ignore` or `archive`), and regex preview.
- Per-scan enablement remains local to the dialog and does not mutate saved rule defaults.

Rule editor:

- Inline form for label, regex, and action.
- Saves to app-level custom rules.
- Shows validation/save errors near the editor.
- Preserves rollback behavior if save fails.

Progress and cancellation:

- Progress panel shows files processed, bytes processed, bytes stored, duplicates, saved bytes, and current path.
- Cancel remains visible and usable during active scans.
- The primary start action enters loading/disabled state while scanning.

## Workspace Flow

Workspace management becomes a focused dialog with a denser list-first layout.

List view:

- Workspace rows show label, path, tags, scan count, file count, duplicates, and saved bytes.
- Active workspace is visually distinct.
- Delete is destructive and visually separated from selection.

Create workspace:

- Label, tags, and store path fields.
- Existing validation behavior remains.

Import existing store:

- Label and store path fields.
- Loading state while importing.

Config import/export:

- Remain available from the workspace manager.
- Import errors appear in the dialog and do not silently fail.

## File, Details, And Stats Views

File browser:

- Keep tree behavior and scan-into action.
- Improve selected/hover states with DaisyUI/Tailwind classes.
- Keep fixed tree column width on desktop.
- Add stable overflow handling and intentional path truncation.

Details:

- Selected file view should use a consistent inspector layout.
- Metadata should be grouped using dense stat/list primitives.
- Duplicate-related states should use semantic color, not only text.

Stats:

- Reuse `UiStat` for high-level totals.
- Keep charts/tables readable within the compact console style.
- Avoid oversized dashboard cards.

## Error Handling

Use consistent DaisyUI alert styling.

- Scan errors render inside `ScanDialog`.
- Custom rule save errors render near `CustomScanRuleEditor`.
- Workspace errors render inside `WorkspaceManagerDialog`.
- Empty states use `UiEmptyState`.

Long error messages must wrap within the dialog and never overflow horizontally.

## Accessibility

- Dialogs must use `role="dialog"`, `aria-modal="true"`, labelled titles, and keyboard-friendly controls.
- Buttons with icon-only presentation need accessible labels and tooltips or titles.
- Form controls need visible labels or `aria-label` when compact.
- Disabled/loading states must be clear.
- Focus outlines should remain visible under DaisyUI Night.

## Testing And Verification

Required checks:

- `npm --prefix app run check`
- `npm --prefix app run build`
- Rust checks only if backend-facing TypeScript/Rust contracts are changed.

Visual verification:

- Run the local app.
- Check desktop and narrow viewport screenshots.
- Verify DaisyUI/Tailwind styles load.
- Verify the top bar, file browser, details view, stats view, workspace dialog, scan dialog, scan-rule editor, progress panel, and cancel button are usable without overlap.
- Verify long paths and regex patterns truncate or wrap intentionally.

Note: The requested `impeccable` skill was not available in the registered skill/tool list for this session. Implementation should use the available local visual verification tooling and browser checks after the plan is approved.

## Implementation Notes

- Expect network/package installation for Tailwind and DaisyUI.
- Keep commits scoped: dependency setup, primitive components, app shell, scan/workspace flows, existing component restyling, verification fixes.
- Avoid committing `.superpowers/` browser-companion artifacts.
