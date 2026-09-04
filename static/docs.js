// Start with compact navigation on small screens. Native details keeps the
// toggle keyboard-accessible and the links usable without JavaScript.
const narrowDocs = window.matchMedia("(max-width: 860px)");
function fitDocsNavigation() {
  document.querySelectorAll(".doc-navigation").forEach((navigation) => {
    navigation.open = !narrowDocs.matches;
  });
}
fitDocsNavigation();
narrowDocs.addEventListener("change", fitDocsNavigation);

// Keep the contents aligned with the last heading to cross the reading line.
// Reading positions on each frame also handles nested headings and long sections.
const sections = Array.from(document.querySelectorAll('#TableOfContents a[href^="#"]'))
  .map((link) => ({
    link,
    heading: document.getElementById(decodeURIComponent(link.hash.slice(1))),
  }))
  .filter(({ heading }) => heading);

if (sections.length) {
  let currentLink;
  let pending = false;
  function updateCurrentSection() {
    pending = false;
    const readingLine = Math.min(96, window.innerHeight * 0.2);
    let current = sections[0];
    for (const section of sections) {
      if (section.heading.getBoundingClientRect().top > readingLine) break;
      current = section;
    }
    // Short final sections may never reach the reading line.
    if (window.scrollY > 0 && window.scrollY + window.innerHeight >= document.documentElement.scrollHeight - 2) {
      current = sections[sections.length - 1];
    }
    if (current.link === currentLink) return;
    currentLink?.classList.remove("active");
    currentLink?.removeAttribute("aria-current");
    currentLink = current.link;
    currentLink.classList.add("active");
    currentLink.setAttribute("aria-current", "location");

    // Scroll only the desktop sidebar, never the article or collapsed mobile menu.
    const sidebar = currentLink.closest(".toc");
    if (sidebar && !narrowDocs.matches) {
      const bounds = sidebar.getBoundingClientRect();
      const item = currentLink.getBoundingClientRect();
      if (item.top < bounds.top || item.bottom > bounds.bottom) {
        sidebar.scrollTop += item.top - bounds.top - sidebar.clientHeight / 2;
      }
    }
  }
  function scheduleSectionUpdate() {
    if (pending) return;
    pending = true;
    window.requestAnimationFrame(updateCurrentSection);
  }
  window.addEventListener("scroll", scheduleSectionUpdate, { passive: true });
  window.addEventListener("resize", scheduleSectionUpdate);
  window.addEventListener("hashchange", scheduleSectionUpdate);
  window.addEventListener("load", scheduleSectionUpdate);
  updateCurrentSection();
}

// Enhance installation alternatives into keyboard-accessible tabs. Without
// JavaScript, every option remains visible with its own label.
document.querySelectorAll("[data-tabs]").forEach((group, groupIndex) => {
  const panels = Array.from(group.querySelectorAll("[data-tab]"));
  if (!panels.length) return;
  const tablist = document.createElement("div");
  tablist.className = "install-tab-list";
  tablist.setAttribute("role", "tablist");
  tablist.setAttribute("aria-label", group.getAttribute("aria-label"));
  const tabs = panels.map((panel, index) => {
    const id = `install-${groupIndex}-${index}`;
    const tab = document.createElement("button");
    tab.type = "button";
    tab.id = `${id}-tab`;
    tab.textContent = panel.dataset.tab;
    tab.setAttribute("role", "tab");
    tab.setAttribute("aria-controls", `${id}-panel`);
    panel.id = `${id}-panel`;
    panel.setAttribute("role", "tabpanel");
    panel.setAttribute("aria-labelledby", tab.id);
    panel.tabIndex = 0;
    panel.querySelector(".tab-fallback-title").hidden = true;
    tablist.append(tab);
    return tab;
  });
  function select(index, focus = false) {
    tabs.forEach((tab, i) => {
      tab.setAttribute("aria-selected", String(i === index));
      tab.tabIndex = i === index ? 0 : -1;
      panels[i].hidden = i !== index;
    });
    if (focus) tabs[index].focus();
    window.dispatchEvent(new Event("resize"));
  }
  tabs.forEach((tab, index) => {
    tab.addEventListener("click", () => select(index));
    tab.addEventListener("keydown", (event) => {
      let next;
      if (event.key === "ArrowRight") next = (index + 1) % tabs.length;
      else if (event.key === "ArrowLeft") next = (index - 1 + tabs.length) % tabs.length;
      else if (event.key === "Home") next = 0;
      else if (event.key === "End") next = tabs.length - 1;
      else return;
      event.preventDefault();
      select(next, true);
    });
  });
  group.prepend(tablist);
  select(0);
});

// Copy only the code, preserving newlines and excluding the button label.
document.querySelectorAll("pre > code").forEach((code) => {
  const pre = code.parentElement;
  const wrapper = document.createElement("div");
  wrapper.className = "code-block";
  const toolbar = document.createElement("div");
  toolbar.className = "code-toolbar";
  const status = document.createElement("span");
  status.className = "copy-status";
  status.setAttribute("role", "status");
  const button = document.createElement("button");
  button.type = "button";
  button.className = "copy-code";
  button.textContent = "Copy";
  button.setAttribute("aria-label", "Copy code");
  let reset;
  button.addEventListener("click", async () => {
    clearTimeout(reset);
    try {
      await navigator.clipboard.writeText(code.textContent);
      button.textContent = "Copied";
      status.textContent = "Code copied to clipboard.";
    } catch {
      const range = document.createRange();
      range.selectNodeContents(code);
      const selection = window.getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
      status.textContent = "Copy unavailable. Code selected; use your copy shortcut.";
    }
    reset = setTimeout(() => {
      button.textContent = "Copy";
      status.textContent = "";
    }, 4000);
  });
  pre.before(wrapper);
  toolbar.append(status, button);
  wrapper.append(toolbar, pre);
});
// The toolbar changes heading positions on short pages.
window.dispatchEvent(new Event("resize"));
