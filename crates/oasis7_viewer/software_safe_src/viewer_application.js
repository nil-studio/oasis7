import { createViewerRenderHookRegistry } from "./viewer_render_hook_registry.js";

// Application lifetime is independent of attached presentation resources. Domain
// owners retain transport, auth, quote and recovery state behind their narrow APIs.
export function createViewerApplication({ coordinateState, bootstrap, onBootstrapError }) {
  const views = createViewerRenderHookRegistry();
  let bootstrapped = false;
  let bootstrapPromise = null;

  // Only the notification registry runs here. Subscribers retain their existing
  // safety cleanup and authorized read recovery responsibilities.
  function publish(invalidation) {
    views.invoke(invalidation);
  }
  function coordinateAndPublish(invalidation) {
    coordinateState();
    publish(invalidation);
  }
  function start() {
    if (bootstrapped) return bootstrapPromise;
    bootstrapped = true;
    bootstrapPromise = bootstrap().catch((error) => {
      onBootstrapError(error);
      coordinateAndPublish();
      throw error;
    });
    return bootstrapPromise;
  }
  return {
    start,
    publish,
    coordinateAndPublish,
    setView: (nextHook) => views.set(nextHook),
    attachView: (nextHook) => views.subscribe(nextHook),
  };
}
