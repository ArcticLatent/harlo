// The home page's motion: sections rise into view as you reach them, the top
// bar's rule shows once the page has scrolled, and its links mark the section
// you're in. Without this script, or with
// reduced motion asked for, everything is simply there.
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

  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
  if ("IntersectionObserver" in window && !reduced) {
    const io = new IntersectionObserver((entries) => {
      // What comes into view together arrives one after another.
      let i = 0;
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        e.target.style.setProperty("--delay", `${Math.min(i++, 5) * 70}ms`);
        e.target.classList.add("in");
        io.unobserve(e.target);
      }
    }, { rootMargin: "0px 0px -8% 0px" });

    const items = document.querySelectorAll(
      "section .kicker, section h2, .section-lead, .points, .wide-shot, .specs > div, .split > :not(.phones), .phones img, .themes figure, .closing p, .closing .cta"
    );
    for (const el of items) {
      // Already on screen: leave it be rather than hide it and bring it back.
      if (el.getBoundingClientRect().top < innerHeight) continue;
      el.classList.add("reveal");
      io.observe(el);
    }
  }
}
