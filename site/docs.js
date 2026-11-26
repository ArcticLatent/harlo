// The documentation: the list at the side marks the part you're reading.
{
  const links = new Map([...document.querySelectorAll(".toc a")].map((a) => [a.getAttribute("href").slice(1), a]));
  if ("IntersectionObserver" in window && links.size) {
    const spy = new IntersectionObserver((entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        for (const a of links.values()) a.removeAttribute("aria-current");
        links.get(e.target.id)?.setAttribute("aria-current", "true");
      }
    }, { rootMargin: "-20% 0px -70% 0px" });
    for (const id of links.keys()) {
      const part = document.getElementById(id);
      if (part) spy.observe(part);
    }
  }
}
