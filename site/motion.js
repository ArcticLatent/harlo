// The home page's motion: sections rise into view as you reach them, and the top
// bar's rule shows once the page has scrolled. Without this script, or with
// reduced motion asked for, everything is simply there.
{
  const bar = document.querySelector("header.top");
  const onScroll = () => bar.classList.toggle("at-top", scrollY < 8);
  addEventListener("scroll", onScroll, { passive: true });
  onScroll();

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
      "section .kicker, section h2, .section-lead, .card, .split > :not(.phones), .phones img, .themes figure, .closing p, .closing .cta"
    );
    for (const el of items) {
      // Already on screen: leave it be rather than hide it and bring it back.
      if (el.getBoundingClientRect().top < innerHeight) continue;
      el.classList.add("reveal");
      io.observe(el);
    }
  }
}
