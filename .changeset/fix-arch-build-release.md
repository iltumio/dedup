---
default: patch
---

# Fix Arch Linux package build in the release pipeline

The Arch build job now installs pnpm via npm instead of corepack, which is no
longer bundled with Arch's `nodejs` package. This unblocks the release workflow
so Arch packages are built and published again.
