const previewData = {
  scan: `Summary
Total found     3.21 GB   1,243 files
Safe to clean   2.83 GB   default filter
At risk         382 MB    opt in with --max-risk high
Ignored         0 B

Path            Size     Rule          Risk
target/debug/   1.48 GB  rust/target   Med
.next/cache/    512 MB   web/cache     Low
dist/           146 MB   web/dist      Med
.pytest_cache/  35 MB    py/cache      Low
node_modules/   382 MB   node/deps     High`,

  plan: `Writing cleanup plan
Output          plan.json
Schema          cleanup-plan/v1
Candidates      4 selected
Dry run         true

Plan preview
1. target/debug/      1.48 GB   rust/cargo-target
2. .next/cache/       512 MB    web/next-cache
3. dist/              146 MB    web/dist
4. .pytest_cache/     35 MB     python/pytest-cache

No filesystem changes were made.`,

  trash: `Applying plan.json with --trash
Batch           2026-04-29T13-30-00Z
Trash root      ~/Library/Application Support/dev-cleaner/trash

Moved
target/debug/       -> batch/target-debug
.next/cache/        -> batch/next-cache
dist/               -> batch/dist
.pytest_cache/      -> batch/pytest-cache

Undo with: dev-cleaner undo --batch 2026-04-29T13-30-00Z`,

  undo: `Restoring latest trash batch
Batch           2026-04-29T13-30-00Z
Entries         4

Restored
target/debug/
.next/cache/
dist/
.pytest_cache/

Restore complete.`
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
