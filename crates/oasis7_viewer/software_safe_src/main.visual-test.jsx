// Direct offline fixtures own explicit local configuration. Live test pages
// require the launcher's trusted configuration exactly like release pages.
if (new URLSearchParams(location.search).get("connect") === "0" && !document.querySelector("#oasis7-viewer-runtime-config")) {
  const config = document.createElement("script");
  config.id = "oasis7-viewer-runtime-config";
  config.type = "application/json";
  config.textContent = JSON.stringify({ deploymentMode: "trusted_local_only", viewerWsEndpoint: "ws://127.0.0.1:9001/" });
  document.head.append(config);
}
Promise.all([import("./viewer_app.jsx"), import("./viewer_visual_test_assembly.js")]).then(([app, assembly]) => {
  app.configureViewerVisualTest(assembly.viewerVisualTestAdapter);
  const lifecycle = { count: 0, dispose: null };
  const mount = () => {
    lifecycle.dispose = app.mountViewerApp();
    lifecycle.count += 1;
    return lifecycle.count;
  };
  mount();
  // P07 needs the real disposer/mount pair. Keep the lifecycle control in the
  // test-only entry and expose it only to explicit test_api pages; the release
  // entry never imports this module.
  if (new URLSearchParams(location.search).get("test_api") === "1") {
    Object.defineProperty(window, "__BFR_REMOUNT__", {
      configurable: true,
      value: Object.freeze({
        mountCount: () => lifecycle.count,
        dispose() {
          const dispose = lifecycle.dispose;
          lifecycle.dispose = null;
          if (dispose) dispose();
        },
        mount() {
          if (lifecycle.dispose) throw new Error("viewer is already mounted");
          return mount();
        },
      }),
    });
  }
});
