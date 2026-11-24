// The theme: the system's, unless the top bar's button picked one (remembered
// in this browser). Runs in <head>, so the page is drawn in it from the start.
{
  const root = document.documentElement;
  root.classList.add("js");
  try {
    const saved = localStorage.getItem("theme");
    if (saved === "light" || saved === "dark") root.dataset.theme = saved;
  } catch {}

  document.addEventListener("click", (e) => {
    if (!e.target.closest("[data-theme-toggle]")) return;
    const light = root.dataset.theme
      ? root.dataset.theme === "light"
      : matchMedia("(prefers-color-scheme: light)").matches;
    root.dataset.theme = light ? "dark" : "light";
    try { localStorage.setItem("theme", root.dataset.theme); } catch {}
  });
}
