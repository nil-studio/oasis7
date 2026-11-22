(() => {
  document.documentElement.setAttribute("data-js", "true");
  const LANGUAGE_REDIRECT_KEY = "oasis7_pages_lang_redirect_done_v1";
  const LANGUAGE_MANUAL_CHOICE_KEY = "oasis7_pages_lang_manual_choice_v1";
  const releaseRefreshCallbacks = [];

  let releaseCandidate = null;
  const releaseAssetUrl = (fallbackUrl) => window.oasis7Release.assetUrl(releaseCandidate, fallbackUrl);
  const setDownloadLink = (node, url) => {
    if (!node) return;
    if (url) {
      node.setAttribute("href", url);
      node.removeAttribute("aria-disabled");
    } else {
      node.removeAttribute("href");
      node.setAttribute("aria-disabled", "true");
    }
  };

  const safeGetStorage = (key) => {
    try {
      return window.localStorage.getItem(key);
    } catch {
      return null;
    }
  };

  const safeSetStorage = (key, value) => {
    try {
      window.localStorage.setItem(key, value);
    } catch {
      return;
    }
  };

  const resolvePreferredLanguage = () => {
    const browserLanguages = Array.isArray(window.navigator.languages)
      ? window.navigator.languages
      : [];
    const firstLanguage = browserLanguages.find(
      (lang) => typeof lang === "string" && lang.trim().length > 0,
    );
    return String(firstLanguage || window.navigator.language || "").toLowerCase();
  };

  const isDocsPath = (pathname) => /\/doc\/(?:cn|en)\//.test(pathname);

  const maybeRedirectByLanguageOnFirstVisit = () => {
    const manualChoice = safeGetStorage(LANGUAGE_MANUAL_CHOICE_KEY);
    if (manualChoice === "zh" || manualChoice === "en") {
      return;
    }

    if (safeGetStorage(LANGUAGE_REDIRECT_KEY) === "1") {
      return;
    }

    const targetAttribute = resolvePreferredLanguage().startsWith("en") ? "data-locale-en" : "data-locale-zh";
    const declared = document.documentElement.getAttribute(targetAttribute);
    if (!declared || isDocsPath(window.location.pathname)) return;
    const target = new URL(declared, window.location.href);
    if (target.origin !== window.location.origin) return;
    safeSetStorage(LANGUAGE_REDIRECT_KEY, "1");
    if (target.pathname === window.location.pathname) return;
    target.search = window.location.search;
    target.hash = window.location.hash;
    window.location.replace(target.href);
  };

  const bindLanguageChoicePersistence = () => {
    document.querySelectorAll("[data-lang-choice]").forEach((link) => {
      link.addEventListener("click", () => {
        const choice = link.getAttribute("data-lang-choice");
        if (choice === "zh" || choice === "en") {
          safeSetStorage(LANGUAGE_MANUAL_CHOICE_KEY, choice);
          safeSetStorage(LANGUAGE_REDIRECT_KEY, "1");
        }
      });
    });
  };

  const bindSectionReveal = () => {
    const revealNodes = Array.from(document.querySelectorAll("[data-reveal]"));
    if (!revealNodes.length) {
      return;
    }

    const revealAll = () => {
      revealNodes.forEach((node) => node.classList.add("revealed"));
    };

    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      revealAll();
      return;
    }

    const observer = new IntersectionObserver(
      (entries, obs) => {
        entries.forEach((entry) => {
          if (!entry.isIntersecting) {
            return;
          }
          entry.target.classList.add("revealed");
          obs.unobserve(entry.target);
        });
      },
      {
        threshold: 0.18,
      },
    );

    revealNodes.forEach((node) => observer.observe(node));

    window.setTimeout(() => {
      revealAll();
      observer.disconnect();
    }, 1800);
  };

  const bindCounters = () => {
    const counters = Array.from(document.querySelectorAll("[data-counter-target]"));
    if (!counters.length) {
      return;
    }

    const prefersReducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

    const renderFinal = (node) => {
      const target = Number(node.getAttribute("data-counter-target") || "0");
      node.textContent = String(Math.max(0, Math.round(target)));
    };

    if (prefersReducedMotion) {
      counters.forEach(renderFinal);
      return;
    }

    const animate = (node) => {
      const target = Number(node.getAttribute("data-counter-target") || "0");
      const safeTarget = Math.max(0, Math.round(target));
      const startedAt = performance.now();
      const duration = Math.min(1400, 520 + safeTarget * 20);

      const tick = (now) => {
        const progress = Math.min(1, (now - startedAt) / duration);
        const eased = 1 - Math.pow(1 - progress, 3);
        node.textContent = String(Math.round(safeTarget * eased));
        if (progress < 1) {
          window.requestAnimationFrame(tick);
        }
      };

      window.requestAnimationFrame(tick);
    };

    const observer = new IntersectionObserver(
      (entries, obs) => {
        entries.forEach((entry) => {
          if (!entry.isIntersecting) {
            return;
          }
          animate(entry.target);
          obs.unobserve(entry.target);
        });
      },
      {
        threshold: 0.4,
      },
    );

    counters.forEach((node) => observer.observe(node));

    window.setTimeout(() => {
      counters.forEach((node) => {
        if (node.textContent === "0") {
          const target = Number(node.getAttribute("data-counter-target") || "0");
          node.textContent = String(Math.max(0, Math.round(target)));
        }
      });
      observer.disconnect();
    }, 2200);
  };

  const bindActiveNav = () => {
    const nav = document.querySelector("[data-section-nav]");
    if (!nav) {
      return;
    }

    const links = Array.from(nav.querySelectorAll("a[href^='#']"));
    if (!links.length) {
      return;
    }

    const linkMap = new Map();
    const sections = [];

    links.forEach((link) => {
      const id = link.getAttribute("href")?.slice(1);
      if (!id) {
        return;
      }
      const section = document.getElementById(id);
      if (!section) {
        return;
      }
      linkMap.set(section, link);
      sections.push(section);
    });

    const setActiveLink = (link) => {
      links.forEach((node) => {
        if (node === link) {
          node.classList.add("active");
        } else {
          node.classList.remove("active");
        }
      });
    };

    if (links[0]) {
      setActiveLink(links[0]);
    }

    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((entry) => entry.isIntersecting)
          .sort((first, second) => second.intersectionRatio - first.intersectionRatio);
        if (!visible.length) {
          return;
        }
        const current = visible[0].target;
        const link = linkMap.get(current);
        if (link) {
          setActiveLink(link);
        }
      },
      {
        rootMargin: "-28% 0px -54% 0px",
        threshold: [0.2, 0.35, 0.5],
      },
    );

    sections.forEach((section) => observer.observe(section));
  };

  const bindTimelineFilters = () => {
    const controls = document.querySelector("[data-timeline-controls]");
    const timeline = document.querySelector("[data-timeline-group]");
    if (!controls || !timeline) {
      return;
    }

    const buttons = Array.from(controls.querySelectorAll("[data-timeline-filter]"));
    const items = Array.from(timeline.querySelectorAll("[data-timeline-state]"));
    if (!buttons.length || !items.length) {
      return;
    }

    const applyFilter = (filter) => {
      items.forEach((item) => {
        const state = item.getAttribute("data-timeline-state") || "";
        const visible = filter === "all" || state === filter;
        item.setAttribute("data-hidden", visible ? "false" : "true");
      });

      buttons.forEach((button) => {
        const isActive = button.getAttribute("data-timeline-filter") === filter;
        button.classList.toggle("is-active", isActive);
        button.setAttribute("aria-pressed", isActive ? "true" : "false");
      });
    };

    buttons.forEach((button) => {
      button.addEventListener("click", () => {
        const filter = button.getAttribute("data-timeline-filter") || "all";
        applyFilter(filter);
      });
    });

    applyFilter("all");
  };

  const bindStoryPathHighlight = () => {
    const steps = Array.from(document.querySelectorAll("[data-story-step]"));
    if (!steps.length) {
      return;
    }

    const setActiveStep = (active) => {
      steps.forEach((step) => {
        step.classList.toggle("is-active", step === active);
      });
    };

    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      if (steps[0]) {
        setActiveStep(steps[0]);
      }
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((entry) => entry.isIntersecting)
          .sort((first, second) => second.intersectionRatio - first.intersectionRatio);
        if (!visible.length) {
          return;
        }
        setActiveStep(visible[0].target);
      },
      {
        threshold: [0.35, 0.5, 0.7],
        rootMargin: "-10% 0px -18% 0px",
      },
    );

    steps.forEach((step) => observer.observe(step));

    if (steps[0]) {
      setActiveStep(steps[0]);
    }
  };

  const escapeHtml = (value) =>
    String(value)
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;")
      .replaceAll("'", "&#39;");

  const renderInlineMarkdown = (value) => {
    let html = escapeHtml(value);
    const codeTokens = [];
    html = html.replace(/`([^`]+)`/g, (_match, code) => {
      const token = `@@CODE${codeTokens.length}@@`;
      codeTokens.push(`<code>${code}</code>`);
      return token;
    });
    html = html.replace(/\[([^\]]+)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g, (_match, label, href) => {
      const url = String(href || "").trim();
      if (!url || /^(?:javascript|data):/i.test(url)) {
        return label;
      }
      return `<a href="${url}">${label}</a>`;
    });
    html = html.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
    html = html.replace(/\*([^*]+)\*/g, "<em>$1</em>");
    codeTokens.forEach((tokenHtml, index) => {
      html = html.replaceAll(`@@CODE${index}@@`, tokenHtml);
    });
    return html;
  };

  const renderMarkdown = (markdown) => {
    const lines = String(markdown || "").replace(/\r\n?/g, "\n").split("\n");
    const html = [];
    let paragraph = [];
    let listType = "";
    let inBlockquote = false;
    let inFence = false;
    let fenceLines = [];

    const closeParagraph = () => {
      if (!paragraph.length) {
        return;
      }
      html.push(`<p>${renderInlineMarkdown(paragraph.join(" "))}</p>`);
      paragraph = [];
    };

    const closeList = () => {
      if (!listType) {
        return;
      }
      html.push(`</${listType}>`);
      listType = "";
    };

    const closeBlockquote = () => {
      if (!inBlockquote) {
        return;
      }
      closeParagraph();
      closeList();
      html.push("</blockquote>");
      inBlockquote = false;
    };

    const openList = (nextType) => {
      if (listType === nextType) {
        return;
      }
      closeList();
      html.push(`<${nextType}>`);
      listType = nextType;
    };

    lines.forEach((line) => {
      if (line.trim().startsWith("```")) {
        closeParagraph();
        closeList();
        closeBlockquote();
        if (inFence) {
          html.push(`<pre><code>${escapeHtml(fenceLines.join("\n"))}</code></pre>`);
          fenceLines = [];
          inFence = false;
        } else {
          inFence = true;
        }
        return;
      }

      if (inFence) {
        fenceLines.push(line);
        return;
      }

      const trimmed = line.trim();
      if (!trimmed) {
        closeParagraph();
        closeList();
        closeBlockquote();
        return;
      }

      const blockquoteMatch = trimmed.match(/^>\s?(.*)$/);
      if (blockquoteMatch) {
        closeParagraph();
        closeList();
        if (!inBlockquote) {
          html.push("<blockquote>");
          inBlockquote = true;
        }
        paragraph.push(blockquoteMatch[1]);
        return;
      }

      closeBlockquote();

      const headingMatch = trimmed.match(/^(#{1,4})\s+(.+)$/);
      if (headingMatch) {
        closeParagraph();
        closeList();
        const level = headingMatch[1].length;
        html.push(`<h${level}>${renderInlineMarkdown(headingMatch[2])}</h${level}>`);
        return;
      }

      const unorderedMatch = trimmed.match(/^[-*]\s+(.+)$/);
      if (unorderedMatch) {
        closeParagraph();
        openList("ul");
        html.push(`<li>${renderInlineMarkdown(unorderedMatch[1])}</li>`);
        return;
      }

      const orderedMatch = trimmed.match(/^\d+\.\s+(.+)$/);
      if (orderedMatch) {
        closeParagraph();
        openList("ol");
        html.push(`<li>${renderInlineMarkdown(orderedMatch[1])}</li>`);
        return;
      }

      closeList();
      paragraph.push(trimmed);
    });

    if (inFence) {
      html.push(`<pre><code>${escapeHtml(fenceLines.join("\n"))}</code></pre>`);
    }
    closeParagraph();
    closeList();
    closeBlockquote();
    return html.join("\n");
  };

  const bindStoryReader = () => {
    const reader = document.querySelector("[data-story-reader]");
    if (!reader || typeof window.fetch !== "function") {
      return;
    }

    const buttons = Array.from(reader.querySelectorAll("[data-story-src]"));
    const status = reader.querySelector("[data-story-status]");
    const content = reader.querySelector("[data-story-content]");
    const panel = reader.querySelector(".reader-panel");
    if (!buttons.length || !status || !content || !panel) {
      return;
    }

    const cache = new Map();
    let requestId = 0;

    const setStatus = (message, state) => {
      status.textContent = message;
      reader.setAttribute("data-reader-state", state);
      panel.setAttribute("aria-busy", state === "loading" ? "true" : "false");
    };

    const setActiveButton = (activeButton) => {
      buttons.forEach((button) => {
        const isActive = button === activeButton;
        button.classList.toggle("is-active", isActive);
        button.setAttribute("aria-pressed", isActive ? "true" : "false");
      });
    };

    const loadChapter = async (button) => {
      const src = String(button.getAttribute("data-story-src") || "").trim();
      if (!src) {
        return;
      }

      const currentRequest = requestId + 1;
      requestId = currentRequest;
      setActiveButton(button);
      content.innerHTML = "";
      const original = reader.querySelector("[data-story-original]");
      if (original) original.setAttribute("href", src);
      setStatus(`正在加载：${button.textContent.trim()}`, "loading");

      try {
        const markdown = cache.has(src)
          ? cache.get(src)
          : await window.fetch(src).then((response) => {
              if (!response.ok) {
                throw new Error(`HTTP ${response.status}`);
              }
              return response.text();
            });

        if (currentRequest !== requestId) {
          return;
        }
        cache.set(src, markdown);

        if (!String(markdown).trim()) {
          setStatus(`当前章节暂无内容：${button.textContent.trim()}。可查看原文或重试。`, "empty");
          return;
        }
        content.innerHTML = renderMarkdown(markdown);
        setStatus(`已加载：${button.textContent.trim()}`, "ready");
      } catch (error) {
        if (currentRequest !== requestId) {
          return;
        }
        const localHint = window.location.protocol === "file:"
          ? "请通过 GitHub Pages 或本地静态服务器打开页面；浏览器通常会拦截 file:// 下的章节加载。"
          : "请确认当前静态站点服务仍在运行，或打开正文索引查看原始 markdown。";
        content.innerHTML = "";
        setStatus(`加载失败：${button.textContent.trim()}。${localHint}`, "error");
      }
    };

    buttons.forEach((button) => {
      button.setAttribute("aria-pressed", "false");
      button.addEventListener("click", () => loadChapter(button));
    });

    const retry = reader.querySelector("[data-story-retry]");
    if (retry) retry.addEventListener("click", () => {
      const selected = buttons.find((button) => button.getAttribute("aria-pressed") === "true");
      if (!selected) return;
      cache.delete(selected.getAttribute("data-story-src"));
      loadChapter(selected);
    });

    const defaultSrc = String(reader.getAttribute("data-story-default") || "").trim();
    const initialButton =
      buttons.find((button) => button.getAttribute("data-story-src") === defaultSrc) || buttons[0];
    document.querySelectorAll("[data-story-start]").forEach((link) => {
      link.addEventListener("click", (event) => {
        if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
        loadChapter(initialButton);
      });
    });
    loadChapter(initialButton);
  };

  const bindLatestReleaseMeta = () => {
    const tagNodes = Array.from(document.querySelectorAll("[data-release-tag]"));
    const dateNodes = Array.from(document.querySelectorAll("[data-release-date]"));
    const notesLinks = Array.from(document.querySelectorAll("[data-release-notes-link]"));
    if (!tagNodes.length && !dateNodes.length && !notesLinks.length) {
      return;
    }
    if (typeof window.fetch !== "function") {
      return;
    }

    const apiUrl = window.oasis7Release.latestApi;
    const controller = typeof AbortController === "function" ? new AbortController() : null;
    let timeoutId = 0;
    if (controller) {
      timeoutId = window.setTimeout(() => {
        controller.abort();
      }, 4500);
    }

    const requestOptions = {
      headers: {
        Accept: "application/vnd.github+json",
      },
    };
    if (controller) {
      requestOptions.signal = controller.signal;
    }

    window
      .fetch(apiUrl, requestOptions)
      .then((response) => {
        if (!response.ok) {
          throw new Error(`release lookup failed: ${response.status}`);
        }
        return response.json();
      })
      .then((release) => {
        const candidate = window.oasis7Release.fromRelease(release);
        const tagName = candidate.tag;
        const publishedAt = candidate.publishedAt;
        releaseCandidate = candidate;
        tagNodes.forEach((node) => {
          node.textContent = tagName;
        });

        const releaseUrl = candidate.notes;
        notesLinks.forEach((node) => {
          node.setAttribute("href", releaseUrl);
        });

        const pageLang = String(document.documentElement.lang || "").toLowerCase();
        const locale = pageLang.startsWith("zh") ? "zh-CN" : "en-US";
        const formattedDate = new Intl.DateTimeFormat(locale, {
          year: "numeric",
          month: "short",
          day: "2-digit",
        }).format(new Date(publishedAt));

        dateNodes.forEach((node) => {
          const prefix = String(node.getAttribute("data-release-date-prefix") || "").trim();
          node.textContent = prefix ? `${prefix}: ${formattedDate}` : formattedDate;
        });
        document.querySelectorAll('a[href*="oasis7-checksums.txt"]').forEach((node) => {
          setDownloadLink(node, releaseAssetUrl(node.getAttribute("href")));
        });
        document.querySelectorAll("[data-release-status]").forEach((node) => {
          node.textContent = pageLang.startsWith("zh") ? "已读取发行元数据；未代签安装或玩法验收。" : "Release metadata loaded; not installation or gameplay validation.";
        });
        releaseRefreshCallbacks.forEach((refresh) => refresh());
      })
      .catch(() => {
        // Keep static fallback text when request fails.
      })
      .finally(() => {
        if (timeoutId) {
          window.clearTimeout(timeoutId);
        }
      });
  };

  const bindReleaseDownloadSurface = () => {
    const surfaces = Array.from(document.querySelectorAll("[data-download-surface]"));
    if (!surfaces.length) {
      return;
    }

    const pageLang = String(document.documentElement.lang || "").toLowerCase();
    const isZh = pageLang.startsWith("zh");
    const platformLabels = {
      windows: "Windows x64",
      macos: "macOS x64",
      linux: "Linux x64",
    };
    const buildRecommendationText = (platformId, mode) => {
      const platformLabel = platformLabels[platformId] || platformId;
      if (isZh) {
        if (mode === "auto") {
          return {
            badge: "当前设备推荐",
            footnote: `已按当前设备优先选中 ${platformLabel}；如果你要给其他机器下载，可切换上方平台按钮。`,
          };
        }
        if (mode === "manual") {
          return {
            badge: "手动切换平台",
            footnote: `当前显示的是你手动切换后的 ${platformLabel} 主包；如果要恢复自动判断，请刷新页面后按当前设备重新选择。`,
          };
        }
        return {
          badge: "按平台选择主包",
          footnote: `当前无法可靠识别设备平台，默认展示 ${platformLabel} 主包；如果你要给其他机器下载，可切换上方平台按钮。`,
        };
      }

      if (mode === "auto") {
        return {
          badge: "Recommended for this device",
          footnote: `The page auto-selected ${platformLabel} for the current device. Switch platforms above if you are downloading for a different machine.`,
        };
      }
      if (mode === "manual") {
        return {
          badge: "Manually selected platform",
          footnote: `You manually switched to the ${platformLabel} primary package. Refresh the page if you want the surface to follow device detection again.`,
        };
      }
      return {
        badge: "Choose a platform",
        footnote: `The page could not confidently detect the current device platform, so it is showing the ${platformLabel} primary package by default. Switch platforms above if needed.`,
      };
    };

    const detectPreferredPlatform = () => {
      const uaData = window.navigator && window.navigator.userAgentData ? window.navigator.userAgentData : null;
      const userAgent = window.navigator ? String(window.navigator.userAgent || "").toLowerCase() : "";
      const platform = window.navigator ? String(window.navigator.platform || "").toLowerCase() : "";
      const uaPlatform = uaData ? String(uaData.platform || "").toLowerCase() : "";
      const candidateText = [uaPlatform, platform, userAgent]
        .filter(Boolean)
        .join(" ");
      const isMobile =
        (uaData && uaData.mobile === true) ||
        /android|iphone|ipad|ipod|mobile/.test(candidateText);
      // The advertised packages are x64. An explicitly ARM device must not
      // inherit an automatic x64 recommendation from its operating-system name.
      if (isMobile || /\barm(?:64)?\b|aarch64/.test(candidateText)) {
        return "";
      }

      const candidates = [
        uaPlatform,
        platform,
        userAgent,
      ]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();

      if (candidates.includes("mac") || candidates.includes("darwin")) {
        return "macos";
      }
      if (/\bwindows\b|\bwin32\b|\bwin64\b|\bwinnt\b/.test(candidates)) {
        return "windows";
      }
      if (candidates.includes("linux") || candidates.includes("x11")) {
        return "linux";
      }
      return "";
    };

    surfaces.forEach((surface) => {
      const buttons = Array.from(surface.querySelectorAll("[data-download-platform-button]"));
      const sourceNodes = Array.from(surface.querySelectorAll("[data-download-platform-source]"));
      if (!buttons.length || !sourceNodes.length) {
        return;
      }

      const sourceMap = new Map();
      sourceNodes.forEach((node) => {
        const platformId = String(node.getAttribute("data-download-platform-source") || "").trim();
        if (!platformId) {
          return;
        }

        const readText = (selector) => {
          const target = node.querySelector(selector);
          return target ? target.textContent.trim() : "";
        };

        sourceMap.set(platformId, {
          url: String(node.getAttribute("data-download-source-url") || "").trim(),
          title: readText("[data-download-source-title]"),
          copy: readText("[data-download-source-copy]"),
          linkLabel: readText("[data-download-source-link-label]"),
          requirements: readText("[data-download-source-requirements]"),
          install: readText("[data-download-source-install]"),
          trust: readText("[data-download-source-trust]"),
          support: readText("[data-download-source-support]"),
        });
      });

      const badgeNode = surface.querySelector("[data-download-primary-badge]");
      const titleNode = surface.querySelector("[data-download-primary-title]");
      const copyNode = surface.querySelector("[data-download-primary-copy]");
      const linkNode = surface.querySelector("[data-download-primary-link]");
      const requirementsNode = surface.querySelector("[data-download-primary-requirements]");
      const installNode = surface.querySelector("[data-download-primary-install]");
      const trustNode = surface.querySelector("[data-download-primary-trust]");
      const supportNode = surface.querySelector("[data-download-primary-support]");
      const footnoteNode = surface.querySelector("[data-download-primary-footnote]");

      const applyPlatform = (platformId, selectionMode) => {
        const next = sourceMap.get(platformId);
        if (!next) {
          surface.setAttribute("data-download-active-platform", "");
          setDownloadLink(linkNode, "");
          if (titleNode) titleNode.textContent = isZh ? "请选择下载目标平台" : "Choose the target platform";
          if (badgeNode) badgeNode.textContent = isZh ? "未知或不支持的当前设备" : "Unknown or unsupported device";
          if (copyNode) copyNode.textContent = isZh ? "不推荐本设备安装。可为其他受支持的桌面设备手动选择；也可以继续阅读。" : "No package is recommended for this device. You may manually choose a supported desktop target or keep reading.";
          if (footnoteNode) footnoteNode.textContent = isZh ? "请选择目标平台；这里未确认本设备兼容性。" : "Choose a target platform; this device's compatibility is not confirmed.";
          [requirementsNode, installNode, trustNode, supportNode].forEach((node) => { if (node) node.textContent = isZh ? "请先选择目标平台并核对对应发行说明。" : "Choose a target platform and check its release notes first."; });
          buttons.forEach((button) => { button.setAttribute("aria-pressed", "false"); button.classList.remove("is-active"); });
          return;
        }

        surface.setAttribute("data-download-active-platform", platformId);
        surface.setAttribute("data-download-selection-mode", selectionMode);
        buttons.forEach((button) => {
          const isActive = String(button.getAttribute("data-download-platform-button") || "") === platformId;
          button.classList.toggle("is-active", isActive);
          button.setAttribute("aria-pressed", isActive ? "true" : "false");
        });

        const recommendationText = buildRecommendationText(platformId, selectionMode);
        if (badgeNode) {
          badgeNode.textContent = recommendationText.badge;
        }
        if (titleNode && next.title) {
          titleNode.textContent = next.title;
        }
        if (copyNode && next.copy) {
          copyNode.textContent = next.copy;
        }
        if (linkNode && next.url) {
          setDownloadLink(linkNode, releaseAssetUrl(next.url));
        }
        if (linkNode && next.linkLabel) {
          linkNode.textContent = next.linkLabel;
        }
        if (requirementsNode && next.requirements) {
          requirementsNode.textContent = next.requirements;
        }
        if (installNode && next.install) {
          installNode.textContent = next.install;
        }
        if (trustNode && next.trust) {
          trustNode.textContent = next.trust;
        }
        if (supportNode && next.support) {
          supportNode.textContent = next.support;
        }
        if (footnoteNode) {
          footnoteNode.textContent = releaseCandidate && !releaseAssetUrl(next.url)
            ? (isZh ? "本候选缺少此平台资产或 checksum，下载推荐已停止。请查看发行说明或换平台。" : "This candidate lacks the platform asset or checksum. Download recommendation stopped; see release notes or choose another platform.")
            : recommendationText.footnote;
        }
      };

      buttons.forEach((button) => {
        button.addEventListener("click", () => {
          applyPlatform(String(button.getAttribute("data-download-platform-button") || ""), "manual");
        });
      });

      const detected = detectPreferredPlatform();
      const initialPlatform = sourceMap.has(detected) ? detected : "";
      const initialMode = detected && sourceMap.has(detected) ? "auto" : "neutral";
      releaseRefreshCallbacks.push(() => {
        const active = surface.getAttribute("data-download-active-platform");
        const mode = surface.getAttribute("data-download-selection-mode") || initialMode;
        applyPlatform(active, mode);
      });
      applyPlatform(initialPlatform, initialMode);
    });
  };

  document.querySelectorAll(".proof-figure img").forEach((img) => {
    img.addEventListener("error", () => {
      const figure = img.closest("figure");
      if (!figure || figure.querySelector(".media-error")) return;
      const note = document.createElement("p");
      note.className = "media-error";
      note.textContent = document.documentElement.lang.startsWith("zh") ? "参考图不可用；文字与来源仍可阅读。" : "Reference image unavailable; text and source remain readable.";
      figure.append(note);
    });
  });

  maybeRedirectByLanguageOnFirstVisit();
  bindLanguageChoicePersistence();
  bindReleaseDownloadSurface();

  const menu = document.querySelector("[data-menu]");
  const toggle = document.querySelector("[data-menu-toggle]");
  const langToggle = document.querySelector("[data-lang-toggle]");
  const langPopover = document.querySelector("[data-lang-popover]");
  const langItems = Array.from(document.querySelectorAll("[data-lang-item]"));
  const yearNode = document.querySelector("[data-year]");

  if (yearNode) {
    yearNode.textContent = String(new Date().getFullYear());
  }

  if (menu && toggle) {
    const menuId = menu.id || "site-nav-menu";
    if (!menu.id) {
      menu.id = menuId;
    }
    if (!toggle.hasAttribute("aria-controls")) {
      toggle.setAttribute("aria-controls", menu.id);
    }

    const setMenuOpen = (open) => {
      menu.setAttribute("data-open", open ? "true" : "false");
      toggle.setAttribute("aria-expanded", open ? "true" : "false");
    };

    setMenuOpen(menu.getAttribute("data-open") === "true");

    toggle.addEventListener("click", () => {
      const opened = menu.getAttribute("data-open") === "true";
      setMenuOpen(!opened);
    });

    menu.querySelectorAll("a").forEach((link) => {
      link.addEventListener("click", () => {
        setMenuOpen(false);
      });
    });

    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        setMenuOpen(false);
      }
    });
  }

  if (langToggle && langPopover) {
    const closePopover = () => {
      langPopover.setAttribute("data-open", "false");
      langToggle.setAttribute("aria-expanded", "false");
    };

    const openPopover = () => {
      langPopover.setAttribute("data-open", "true");
      langToggle.setAttribute("aria-expanded", "true");
    };

    langToggle.addEventListener("click", () => {
      const opened = langPopover.getAttribute("data-open") === "true";
      if (opened) {
        closePopover();
      } else {
        openPopover();
      }
    });

    langToggle.addEventListener("keydown", (event) => {
      if (event.key === "ArrowDown" || event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        openPopover();
        if (langItems[0]) {
          langItems[0].focus();
        }
      }
    });

    langItems.forEach((item, index) => {
      item.addEventListener("keydown", (event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          closePopover();
          langToggle.focus();
          return;
        }

        if (event.key === "ArrowDown") {
          event.preventDefault();
          const next = langItems[index + 1] || langItems[0];
          next.focus();
          return;
        }

        if (event.key === "ArrowUp") {
          event.preventDefault();
          const previous = langItems[index - 1] || langItems[langItems.length - 1];
          previous.focus();
        }
      });
    });

    document.addEventListener("click", (event) => {
      const target = event.target;
      if (!(target instanceof Node)) {
        return;
      }
      if (langToggle.contains(target) || langPopover.contains(target)) {
        return;
      }
      closePopover();
    });

    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        closePopover();
      }
    });

    langPopover.querySelectorAll("a").forEach((link) => {
      link.addEventListener("click", () => {
        closePopover();
      });
    });
  }

  for (const [anchor, selector] of [["#download", "[data-preview-details]"], ["#package-download", "[data-preview-details]"], ["#world-laws", "#world-laws > details"], ["#lore", "#lore > details"], ["#case-conditions", "#case-conditions"]]) {
    const previewDetails = document.querySelector(selector);
    if (!previewDetails) continue;
    const target = document.querySelector(anchor);
    let previewLinkHashPending = false;
    const positionTarget = (focus = false) => {
      requestAnimationFrame(() => {
        if (window.location.hash !== anchor || !previewDetails.open || !target) return;
        target.scrollIntoView({ block: "start", behavior: "instant" });
        if (focus) {
          const destination = target.querySelector("h2, [data-download-platform-button], summary") || target;
          if (!destination.matches("button, summary")) destination.setAttribute("tabindex", "-1");
          destination.focus({ preventScroll: true });
        }
      });
    };
    const openPreviewFromHash = () => {
      if (window.location.hash !== anchor) { previewLinkHashPending = false; return; }
      // The link already opened it before native navigation; a later hash event must not undo a quick close.
      if (previewLinkHashPending) { previewLinkHashPending = false; return; }
      previewDetails.open = true;
      positionTarget();
    };
    openPreviewFromHash();
    window.addEventListener("hashchange", openPreviewFromHash);
    document.querySelectorAll(`a[href="${anchor}"]`).forEach((link) => {
      link.addEventListener("click", (event) => {
        if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
        if (window.location.hash !== anchor) previewLinkHashPending = true;
        previewDetails.open = true;
        positionTarget(event.detail === 0);
      });
    });
  }

  bindSectionReveal();
  bindCounters();
  bindActiveNav();
  bindTimelineFilters();
  bindStoryPathHighlight();
  bindStoryReader();
  // Evidence is tied to one recorded release, never inferred from a newer tag or its assets.
  document.querySelectorAll("[data-package-validation-copy]").forEach((node) => {
    const recordedCopy = node.textContent;
    const refresh = () => {
      const tag = node.closest(".preview-validation").querySelector("[data-package-candidate-tag]");
      if (tag) tag.textContent = releaseCandidate ? releaseCandidate.tag : node.dataset.verifiedRelease;
      if (!releaseCandidate || releaseCandidate.tag === node.dataset.verifiedRelease) { node.textContent = recordedCopy; return; }
      node.textContent = document.documentElement.lang.startsWith("zh")
        ? "此下载候选没有完整试玩验证记录；下方六月构建记录只适用于六月版本。"
        : "A complete playtest is not verified for this candidate. The June build record below applies only to the June version.";
    };
    releaseRefreshCallbacks.push(refresh);
    refresh();
  });
  bindLatestReleaseMeta();
})();
