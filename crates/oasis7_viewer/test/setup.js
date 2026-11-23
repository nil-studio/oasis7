import "@testing-library/jest-dom/vitest";
import { webcrypto } from "node:crypto";

if (typeof window !== "undefined" && !window.crypto?.subtle && webcrypto?.subtle) {
  Object.defineProperty(window, "crypto", {
    configurable: true,
    writable: true,
    value: webcrypto,
  });
}

if (typeof window !== "undefined" && typeof window.requestAnimationFrame !== "function") {
  window.requestAnimationFrame = (callback) => setTimeout(() => callback(Date.now()), 0);
}

if (typeof window !== "undefined" && typeof window.cancelAnimationFrame !== "function") {
  window.cancelAnimationFrame = (handle) => clearTimeout(handle);
}

if (typeof window !== "undefined" && typeof window.matchMedia !== "function") {
  window.matchMedia = (query) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener() {},
    removeListener() {},
    addEventListener() {},
    removeEventListener() {},
    dispatchEvent() {
      return false;
    },
  });
}

if (typeof HTMLCanvasElement !== "undefined") {
  HTMLCanvasElement.prototype.getContext = () => null;
}
