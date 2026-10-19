use super::*;

const SEEK_STALL_LIMIT: u64 = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LiveSeekResult {
    pub(super) reached: bool,
    pub(super) current_tick: u64,
}

impl LiveWorld {
    pub(super) fn seek_to_tick(
        &mut self,
        target_tick: u64,
    ) -> Result<LiveSeekResult, ViewerLiveServerError> {
        let current_tick = self.kernel.time();
        if target_tick == current_tick {
            return Ok(LiveSeekResult {
                reached: true,
                current_tick,
            });
        }
        if self.consensus_bridge.is_some() && target_tick < current_tick {
            return Ok(LiveSeekResult {
                reached: false,
                current_tick,
            });
        }

        if target_tick < current_tick {
            self.reset()?;
        }

        let mut stalled_steps = 0_u64;
        while self.kernel.time() < target_tick {
            let tick_before = self.kernel.time();
            let _ = self.step()?;
            let tick_after = self.kernel.time();

            if tick_after == tick_before {
                stalled_steps = stalled_steps.saturating_add(1);
                if stalled_steps >= SEEK_STALL_LIMIT {
                    return Ok(LiveSeekResult {
                        reached: false,
                        current_tick: tick_after,
                    });
                }
            } else {
                stalled_steps = 0;
            }
        }

        Ok(LiveSeekResult {
            reached: true,
            current_tick: self.kernel.time(),
        })
    }
}
