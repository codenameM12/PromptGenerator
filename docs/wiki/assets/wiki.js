// Small enhancements for the offline docs wiki. Everything works without it.
const filter = document.querySelector("#nav-filter");
const links = [...document.querySelectorAll(".nav-link")];

if (filter) {
  filter.addEventListener("input", () => {
    const query = filter.value.trim().toLowerCase();
    for (const link of links) {
      const haystack = `${link.dataset.title ?? ""} ${link.textContent}`.toLowerCase();
      link.hidden = query.length > 0 && !haystack.includes(query);
    }
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "/" && document.activeElement !== filter) {
      event.preventDefault();
      filter.focus();
    }
  });
}

// Highlight the heading currently in view in the right rail.
const headings = [...document.querySelectorAll(".prose h2[id], .prose h3[id]")];
const tocLinks = new Map(
  [...document.querySelectorAll(".toc-link")].map((link) => [link.getAttribute("href"), link]),
);

if (headings.length && "IntersectionObserver" in window) {
  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        for (const link of tocLinks.values()) link.removeAttribute("aria-current");
        tocLinks.get(`#${entry.target.id}`)?.setAttribute("aria-current", "true");
      }
    },
    { rootMargin: "0px 0px -70% 0px", threshold: [0, 1] },
  );
  for (const heading of headings) observer.observe(heading);
}
