/** Motion for the slide-over panel (Web Animations timings). */

/** In: decelerates into place (ease-out quint). */
export const SLIDE_IN = { duration: 240, easing: "cubic-bezier(0.22, 1, 0.36, 1)" };
/** Out: shorter, and moves at once (fast start) so it follows the click. */
export const SLIDE_OUT = { duration: 160, easing: "cubic-bezier(0.32, 0.72, 0, 1)" };
