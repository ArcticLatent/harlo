// The home page's top bar: its rule shows once the page has scrolled, and its
// links mark the section you're in. (The one entrance, the hero's, is CSS.)
{
  const bar = document.querySelector("header.top");
  const onScroll = () => bar.classList.toggle("at-top", scrollY < 8);
  addEventListener("scroll", onScroll, { passive: true });
  onScroll();

  // The top bar marks the section you're reading.
  const links = new Map([...document.querySelectorAll('nav.links a[href^="#"]')].map((a) => [a.getAttribute("href").slice(1), a]));
  if ("IntersectionObserver" in window && links.size) {
    const spy = new IntersectionObserver((entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        for (const a of links.values()) a.removeAttribute("aria-current");
        links.get(e.target.id)?.setAttribute("aria-current", "true");
      }
    }, { rootMargin: "-45% 0px -50% 0px" });
    // (Every section, so one without a link clears the mark.)
    for (const section of document.querySelectorAll("main > section, main > .hero")) spy.observe(section);
  }
}
