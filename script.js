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
