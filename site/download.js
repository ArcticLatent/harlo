// The release's own files (their names carry the version), and the system
// this browser is on. Without either, the links go to the latest release.
(function () {
  const ua = navigator.userAgent;
  const iphone = /iPhone|iPod/.test(ua) || (/iPad|Macintosh/.test(ua) && navigator.maxTouchPoints > 1);
  const linux = /Linux|X11/.test(ua) && !/Android/.test(ua);
  const pick = document.getElementById("pick");
  const say = (title, text, buttons) => {
    document.getElementById("pick-title").textContent = title;
    document.getElementById("pick-text").textContent = text;
    const cta = document.getElementById("pick-cta");
    for (const [label, href, primary] of buttons) {
      const a = document.createElement("a");
      a.className = primary ? "btn primary" : "btn";
      a.href = href;
      a.textContent = label;
      cta.appendChild(a);
    }
    pick.hidden = false;
  };
  if (iphone) {
    say("On an iPhone?", "Get the harlo app, then pair it with harlo on your Linux computer.",
        [["Get it on the App Store", "https://apps.apple.com/app/id6822811546", true]]);
  } else if (!linux) {
    say("harlo is for Linux", "It runs on Linux computers (and their iPhone app). Open this page on your Linux computer, or pick its file below.", []);
  }
  // A distribution in the user agent (some browsers say).
  const distro = /Ubuntu|Debian|Mint/i.test(ua) ? "deb" : /Fedora|SUSE|Red Hat/i.test(ua) ? "rpm" : /Arch|Manjaro/i.test(ua) ? "arch" : "";
  if (linux && distro) document.querySelector(`.system[data-kind="${distro}"]`).setAttribute("data-picked", "");

  fetch("https://api.github.com/repos/ArcticLatent/harlo/releases/latest", { headers: { Accept: "application/vnd.github+json" } })
    .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
    .then((release) => {
      const match = {
        deb: (n) => n.endsWith("_amd64.deb"),
        rpm: (n) => n.endsWith(".x86_64.rpm"),
        arch: (n) => n.endsWith("-x86_64.pkg.tar.zst"),
        appimage: (n) => n.endsWith("-x86_64.AppImage"),
      };
      for (const [kind, is] of Object.entries(match)) {
        const asset = (release.assets || []).find((a) => is(a.name));
        if (!asset) continue;
        document.querySelector(`[data-file="${kind}"]`).href = asset.browser_download_url;
        const name = document.querySelector(`[data-name="${kind}"]`);
        name.textContent = `${asset.name} · ${(asset.size / 1048576).toFixed(0)} MB`;
      }
      const v = (release.tag_name || "").replace(/^v/, "");
      if (v) document.getElementById("version").textContent = "harlo " + v;
    })
    .catch(() => {});
})();
