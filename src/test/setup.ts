/** jsdom gaps that Radix needs (ResizeObserver, pointer capture, scrollIntoView). */
if (typeof window !== "undefined") {
  window.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
  const proto = window.HTMLElement.prototype;
  proto.hasPointerCapture ??= () => false;
  proto.releasePointerCapture ??= () => {};
  proto.scrollIntoView ??= () => {};
}
