const previewData = {
  scan: `$ dev-cleaner scan
Scanning for cleanable directories...
Found 4 potential projects, calculating sizes...

  ✓ Node.js source: builtin /Users/you/acme/web/.next (64.00 MB)
  ✓ Node.js source: builtin /Users/you/acme/web/dist (24.00 MB)
  ✓ Python source: builtin /Users/you/acme/ml/.pytest_cache (6.00 MB)
  ✓ Rust source: builtin /Users/you/acme/api/target (180.00 MB)

✓ 4 cleanable directories found
Total size: 274.00 MB`,

  plan: `$ dev-cleaner plan -o plan.json
Plan file created: plan.json

$ dev-cleaner apply plan.json --dry-run --yes
Applying cleanup plan...
  Plan: plan.json
  Projects: 4 (0 skipped in verify)
  Total size: 274.00 MB
[DRY RUN] Would move to trash: /Users/you/acme/api/target (180.00 MB)
[DRY RUN] Would move to trash: /Users/you/acme/web/.next (64.00 MB)
[DRY RUN] Would move to trash: /Users/you/acme/web/dist (24.00 MB)
[DRY RUN] Would move to trash: /Users/you/acme/ml/.pytest_cache (6.00 MB)

Dry run completed. Nothing was changed.
  Would move to trash: 274.00 MB`,

  trash: `$ dev-cleaner apply plan.json --yes
Applying cleanup plan...
  Plan: plan.json
  Projects: 4 (0 skipped in verify)
  Total size: 274.00 MB

Cleaning completed!
  Cleaned: 4
  Skipped: 0 (0 B)
  Failed: 0
  Moved to trash: 274.00 MB
  Trash batch: 20261103060844-1793686124976164000-99413
  Undo: dev-cleaner undo --batch 20261103060844-1793686124976164000-99413
  Release space: dev-cleaner trash purge --batch 20261103060844-1793686124976164000-99413`,

  undo: `$ dev-cleaner undo
Restoring from trash...
  Trash root: /Users/you/Library/Application Support/dev-cleaner/trash
  Batch: 20261103060844-1793686124976164000-99413

Restore completed!
  Restored: 4
  Skipped: 0
  Failed: 0`
};

const output = document.querySelector("#terminal-output");
const tabs = document.querySelectorAll(".terminal-tab");

function setPreview(name) {
  output.textContent = previewData[name];
  tabs.forEach((tab) => {
    const isActive = tab.dataset.preview === name;
    tab.classList.toggle("active", isActive);
    tab.setAttribute("aria-selected", String(isActive));
  });
}

tabs.forEach((tab) => {
  tab.addEventListener("click", () => setPreview(tab.dataset.preview));
});

document.querySelectorAll("[data-copy]").forEach((button) => {
  button.addEventListener("click", async () => {
    const value = button.dataset.copy;
    try {
      await navigator.clipboard.writeText(value);
      button.dataset.copied = "true";
      button.setAttribute("aria-label", "Copied");
      window.setTimeout(() => {
        button.dataset.copied = "false";
        button.setAttribute("aria-label", `Copy ${value}`);
      }, 1400);
    } catch {
      button.setAttribute("aria-label", value);
    }
  });
});

setPreview("scan");
