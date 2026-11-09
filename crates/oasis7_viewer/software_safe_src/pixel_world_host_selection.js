import { createMemo } from 'solid-js';

// Share the authorization-derived display selection across all marker consumers.
export function createPixelWorldHostSelection(revision, spatial) {
  return createMemo(() => {
    revision();
    // A scope mismatch returns before selection reads its normal signals. Keep
    // the view dependency so a newly authorized source restores those signals.
    spatial.state();
    return spatial.selection();
  });
}
