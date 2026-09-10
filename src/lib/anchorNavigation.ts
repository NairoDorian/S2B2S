// Deep-link navigation inside the settings app: switch to a page, wait for a
// target element to exist, scroll it into view, focus it and pulse a
// highlight ring around it. Ported from the AIVORelay fork's
// `anchorNavigation.ts`; the highlight keyframes live in App.css.

const ANCHOR_HIGHLIGHT_CLASS = "settings-anchor-highlight";
const ANCHOR_HIGHLIGHT_DURATION_MS = 1800;

let activeAnchor: HTMLElement | null = null;
let highlightTimer: number | null = null;
let navigationGeneration = 0;
let pendingRevealTimer: number | null = null;

const prefersReducedMotion = (): boolean =>
  typeof window !== "undefined" &&
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

const highlightAnchor = (anchor: HTMLElement): void => {
  if (activeAnchor && activeAnchor !== anchor) {
    activeAnchor.classList.remove(ANCHOR_HIGHLIGHT_CLASS);
  }

  // Restart the animation even when the same element is targeted twice.
  anchor.classList.remove(ANCHOR_HIGHLIGHT_CLASS);
  void anchor.offsetWidth;
  anchor.classList.add(ANCHOR_HIGHLIGHT_CLASS);
  activeAnchor = anchor;

  if (highlightTimer !== null) {
    window.clearTimeout(highlightTimer);
  }
  highlightTimer = window.setTimeout(() => {
    anchor.classList.remove(ANCHOR_HIGHLIGHT_CLASS);
    if (activeAnchor === anchor) {
      activeAnchor = null;
    }
    highlightTimer = null;
  }, ANCHOR_HIGHLIGHT_DURATION_MS);
};

/** Scroll to an element that is already in the DOM, focus it and highlight it. */
export const scrollAndFocusAnchor = (
  anchor: HTMLElement,
  block: ScrollLogicalPosition = "start",
  isCurrent: () => boolean = () => true,
): void => {
  if (!isCurrent()) return;

  anchor.scrollIntoView({
    behavior: prefersReducedMotion() ? "auto" : "smooth",
    block,
  });

  window.requestAnimationFrame(() => {
    if (isCurrent() && document.contains(anchor)) {
      highlightAnchor(anchor);
      anchor.focus({ preventScroll: true });
    }
  });
};

interface NavigateToSettingsAnchorOptions {
  /** Switch the app to the page that renders `targetId` (usually `setSection`). */
  activateSection: () => void;
  targetId: string;
  /** Element whose presence proves the page has rendered; defaults to the target. */
  readyId?: string;
  /** Scrolled to when the target is missing after the page rendered. */
  fallbackId?: string;
  block?: ScrollLogicalPosition;
}

/**
 * Switch pages and reveal an element by id. The page may need a few frames to
 * mount, so this polls for up to a second; a newer navigation cancels an
 * older one that is still waiting.
 */
export const navigateToSettingsAnchor = ({
  activateSection,
  targetId,
  readyId = targetId,
  fallbackId,
  block = "start",
}: NavigateToSettingsAnchorOptions): void => {
  navigationGeneration += 1;
  const currentGeneration = navigationGeneration;
  const isCurrent = () => currentGeneration === navigationGeneration;

  if (pendingRevealTimer !== null) {
    window.clearTimeout(pendingRevealTimer);
    pendingRevealTimer = null;
  }

  activateSection();

  let attempts = 0;
  const revealAnchor = () => {
    pendingRevealTimer = null;
    if (!isCurrent()) return;

    if (!document.getElementById(readyId)) {
      attempts += 1;
      if (attempts <= 20) {
        pendingRevealTimer = window.setTimeout(revealAnchor, 50);
      }
      return;
    }

    window.requestAnimationFrame(() => {
      if (!isCurrent()) return;
      const target =
        document.getElementById(targetId) ??
        (fallbackId ? document.getElementById(fallbackId) : null);
      if (target) scrollAndFocusAnchor(target, block, isCurrent);
    });
  };

  pendingRevealTimer = window.setTimeout(revealAnchor, 0);
};
