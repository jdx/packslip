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

// Search: load the index on first use, then rank pages by where the words
// match (title, then headings, then body) and show a snippet.
(() => {
  const input = document.getElementById("search-input");
  const list = document.getElementById("search-results");
  if (!input || !list) return;
  let index;
  let loading;
  let selected = -1;
  let generation = 0;

  function load() {
    loading ??= fetch("/search-index.json")
      .then((response) => {
        if (!response.ok) throw new Error(`search index: ${response.status}`);
        return response.json();
      })
      .then((pages) => {
        index = pages.map((page) => ({
          ...page,
          lower: {
            title: page.title.toLowerCase(),
            headings: page.headings.join("\n").toLowerCase(),
            text: page.text.toLowerCase(),
          },
        }));
      })
      .catch((error) => {
        // Let a later focus or query retry instead of keeping the failure.
        loading = undefined;
        throw error;
      });
    return loading;
  }

  function search(query) {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    if (!words.length) return [];
    const results = [];
    for (const page of index) {
      // Matches in the title, then headings, then body, compared in that order.
      let score = [0, 0, 0];
      let matched = true;
      for (const word of words) {
        const inTitle = page.lower.title.includes(word);
        const inHeadings = page.lower.headings.includes(word);
        const inText = page.lower.text.includes(word);
        if (!inTitle && !inHeadings && !inText) { matched = false; break; }
        if (inTitle) score[0]++;
        if (inHeadings) score[1]++;
        if (inText) score[2]++;
      }
      if (matched) results.push({ page, score, words });
    }
    return results.sort((a, b) => b.score[0] - a.score[0] || b.score[1] - a.score[1] || b.score[2] - a.score[2]).slice(0, 8);
  }

  function highlight(parent, text, words) {
    const pattern = new RegExp(`(${words.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")})`, "gi");
    text.split(pattern).forEach((part, i) => {
      if (i % 2) {
        const mark = document.createElement("mark");
        mark.textContent = part;
        parent.append(mark);
      } else {
        parent.append(part);
      }
    });
  }

  function snippet(page, words) {
    const at = words.map((w) => page.lower.text.indexOf(w)).filter((i) => i >= 0);
    const start = Math.max(0, (at.length ? Math.min(...at) : 0) - 40);
    const text = page.text.slice(start, start + 140).trim();
    return (start > 0 ? "… " : "") + text + (start + 140 < page.text.length ? " …" : "");
  }

  function close() {
    generation++;
    list.hidden = true;
    input.setAttribute("aria-expanded", "false");
    selected = -1;
  }

  function select(next) {
    const links = list.querySelectorAll("a");
    if (!links.length) return;
    selected = (next + links.length) % links.length;
    links.forEach((link, i) => link.setAttribute("aria-selected", String(i === selected)));
    links[selected].scrollIntoView({ block: "nearest" });
  }

  async function render() {
    const query = input.value.trim();
    if (!query) return close();
    const mine = ++generation;
    try {
      await load();
    } catch {
      return;
    }
    // Drop a render that the user outran by editing or closing the search.
    if (mine !== generation || input.value.trim() !== query) return;
    list.replaceChildren();
    const results = search(query);
    if (!results.length) {
      const empty = document.createElement("li");
      empty.className = "empty";
      empty.textContent = "No results";
      list.append(empty);
    }
    for (const { page, words } of results) {
      const item = document.createElement("li");
      const link = document.createElement("a");
      link.href = page.url;
      const title = document.createElement("span");
      title.className = "result-title";
      highlight(title, page.title, words);
      const section = document.createElement("span");
      section.className = "result-section";
      section.textContent = page.section === "docs" ? "guide" : page.section === "cli" ? "cli" : "spec";
      const text = document.createElement("span");
      text.className = "result-snippet";
      highlight(text, snippet(page, words), words);
      link.append(title, section, text);
      item.append(link);
      list.append(item);
    }
    selected = -1;
    list.hidden = false;
    input.setAttribute("aria-expanded", "true");
  }

  input.addEventListener("focus", () => load().catch(() => {}));
  input.addEventListener("input", render);
  input.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown") { event.preventDefault(); select(selected + 1); }
    else if (event.key === "ArrowUp") { event.preventDefault(); select(selected < 0 ? -1 : selected - 1); }
    else if (event.key === "Enter") {
      const link = list.hidden ? undefined : list.querySelectorAll("a")[Math.max(selected, 0)];
      if (link) window.location = link.href;
    } else if (event.key === "Escape") { close(); input.blur(); }
  });
  document.addEventListener("click", (event) => {
    if (!event.target.closest(".search")) close();
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "/" && !event.target.closest("input, textarea")) {
      event.preventDefault();
      input.focus();
    }
  });
})();
