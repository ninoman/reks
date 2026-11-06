/**
 * Fades a dialog (and its backdrop) in when it opens and
 * out when it closes. Most dialogs unmount as soon as they close, so Radix
 * can't animate their exit: on unmount a copy of the dialog and backdrop is
 * left in the page for the exit animation, then removed. The copy takes no
 * clicks.
 */
import { useLayoutEffect, useState } from "react";

import { dialogIn, dialogOut } from "../../lib/motion";

const reduced = () => window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;

export function useDialogMotion(): (el: HTMLElement | null) => void {
  const [el, setEl] = useState<HTMLElement | null>(null);
  useLayoutEffect(() => {
    if (el === null || !("animate" in el) || reduced()) return;
    const backdrop = el.previousElementSibling as HTMLElement | null;
    const { duration, easing } = dialogIn();
    el.animate([{ opacity: 0 }, { opacity: 1 }], { duration, easing });
    backdrop?.animate([{ opacity: 0 }, { opacity: 1 }], { duration, easing });

    return () => {
      // Copies taken now; the real nodes are removed right after this cleanup.
      const ghosts = [backdrop, el]
        .filter((n): n is HTMLElement => n !== null)
        .map((n) => {
          const copy = n.cloneNode(true) as HTMLElement;
          copy.style.pointerEvents = "none";
          copy.removeAttribute("id");
          copy.removeAttribute("role");
          copy.setAttribute("aria-hidden", "true");
          return copy;
        });
      queueMicrotask(() => {
        // Still in the page: not a close (a re-render or a dev double mount).
        if (el.isConnected) return;
        const out = dialogOut();
        for (const ghost of ghosts) {
          document.body.append(ghost);
          const anim = ghost.animate([{ opacity: 1 }, { opacity: 0 }], {
            ...out,
            fill: "forwards",
          });
          anim.onfinish = () => ghost.remove();
          anim.oncancel = () => ghost.remove();
          // A hidden page doesn't run animations; never leave a copy behind.
          window.setTimeout(() => ghost.remove(), out.duration + 100);
        }
      });
    };
  }, [el]);
  return setEl;
}
