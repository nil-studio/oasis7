//! Opt-in constructor timings. They measure local phases and make no claim
//! about how time divides among remote verification, transport and fsync.
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

static NEXT_OPEN_ID: AtomicU64 = AtomicU64::new(1);

pub(super) struct OpenTiming {
    open: Option<(u64, Instant)>,
}

impl OpenTiming {
    pub(super) fn new() -> Self {
        Self {
            open: (cfg!(test) || std::env::var_os("OASIS7_FORMAL_OPEN_TIMING").is_some())
                .then(|| (NEXT_OPEN_ID.fetch_add(1, Ordering::Relaxed), Instant::now())),
        }
    }

    pub(super) fn stage(&self, stage: &'static str) -> OpenStageTiming {
        let open = self.open.map(|(id, started)| {
            let now = Instant::now();
            eprintln!(
                "formal_open_timing open_id={id} stage={stage} event=begin elapsed_ms={}",
                now.duration_since(started).as_millis()
            );
            (id, started, now)
        });
        OpenStageTiming { stage, open }
    }

    pub(super) fn flag(&self, name: &'static str, value: bool) {
        if let Some((id, started)) = self.open {
            eprintln!(
                "formal_open_timing open_id={id} {name}={value} elapsed_ms={}",
                started.elapsed().as_millis()
            );
        }
    }
}

pub(super) struct OpenStageTiming {
    stage: &'static str,
    open: Option<(u64, Instant, Instant)>,
}

impl Drop for OpenStageTiming {
    fn drop(&mut self) {
        if let Some((id, open_started, stage_started)) = self.open {
            eprintln!(
                "formal_open_timing open_id={id} stage={} event=end stage_ms={} elapsed_ms={}",
                self.stage,
                stage_started.elapsed().as_millis(),
                open_started.elapsed().as_millis()
            );
        }
    }
}
