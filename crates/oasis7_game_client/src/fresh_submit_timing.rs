//! Operator-only opt-in: `OASIS7_CLIENT_FRESH_SUBMIT_TIMING=1` enables a fresh
//! Client attempt; the default is silent. One JSON record is written to stderr
//! after scope exit: opaque process-local observation_id, bounded events with
//! fixed stage names and monotonic elapsed_us, total scope_elapsed_us (including
//! errors/unwind/polling; not success), and an explicit truncated flag.
//! No request-derived identifiers, body, goal, URL, path or secrets are emitted.
//! Only the armed typed Submit transport records HTTP phases, never ContextRead
//! or Lookup. call_return means the identity HTTP function returned (including
//! errors), not authentication, admission or canonical business success.
//! Missing/truncated traces do not prove that no request or world effect occurred.
//! Recording has bounded in-memory overhead; final output is best-effort and may
//! fail. This is diagnostic evidence, not authority or a deadline replacement.
#![cfg_attr(not(unix), allow(dead_code))]
use serde::Serialize;
use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};
thread_local! {
    static CURRENT: RefCell<Option<State>> = const { RefCell::new(None) };
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}
static NEXT: AtomicU64 = AtomicU64::new(0);
#[derive(Serialize)]
struct Event {
    stage: &'static str,
    elapsed_us: u128,
}
#[derive(Serialize)]
struct State {
    observation_id: String,
    events: Vec<Event>,
    truncated: bool,
    scope_elapsed_us: u128,
    #[serde(skip)]
    start: Option<Instant>,
    #[serde(skip)]
    armed: bool,
    #[serde(skip)]
    submit: bool,
}
pub(crate) struct Scope {
    active: bool,
    saved: Option<State>,
    _thread: PhantomData<Rc<()>>,
}
impl Scope {
    pub(crate) fn operator_enabled() -> Self {
        Self::enter(std::env::var_os("OASIS7_CLIENT_FRESH_SUBMIT_TIMING").is_some_and(|v| v == "1"))
    }
    fn enter(enabled: bool) -> Self {
        let nested = DEPTH.with(|depth| {
            let prior = depth.get();
            depth.set(prior + 1);
            prior != 0
        });
        // Remove the entire outer state while a nested (even disabled) fresh
        // call runs. Its events and transport guards cannot touch the parent.
        let saved = if nested {
            CURRENT.with(|slot| slot.borrow_mut().take())
        } else {
            None
        };
        let active = enabled && !nested;
        if active {
            let id = format!(
                "{}:{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            CURRENT.with(|slot| {
                *slot.borrow_mut() = Some(State {
                    observation_id: blake3::hash(id.as_bytes()).to_hex().to_string(),
                    events: Vec::with_capacity(32),
                    truncated: false,
                    scope_elapsed_us: 0,
                    start: Some(Instant::now()),
                    armed: false,
                    submit: false,
                })
            });
        }
        Self {
            active,
            saved,
            _thread: PhantomData,
        }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get() - 1));
        if !self.active {
            if let Some(saved) = self.saved.take() {
                CURRENT.with(|slot| *slot.borrow_mut() = Some(saved));
            }
            return;
        }
        let state = CURRENT
            .with(|slot| slot.borrow_mut().take())
            .map(|mut state| {
                state.scope_elapsed_us = state.start.unwrap().elapsed().as_micros();
                state
            });
        if let Some(state) = state
            && let Ok(line) = serde_json::to_string(&state)
        {
            {
                use std::io::Write;
                let _ = writeln!(std::io::stderr().lock(), "fresh_submit_timing {line}");
            }
            #[cfg(test)]
            OUTPUT.with(|out| out.borrow_mut().push(line));
        }
    }
}

pub(crate) fn event(stage: &'static str) {
    CURRENT.with(|slot| {
        if let Some(state) = slot.borrow_mut().as_mut() {
            if state.events.len() < 32 {
                state.events.push(Event {
                    stage,
                    elapsed_us: state.start.unwrap().elapsed().as_micros(),
                });
            } else {
                state.truncated = true;
            }
        }
    });
}
pub(crate) fn submit_event(stage: &'static str) {
    if CURRENT.with(|s| s.borrow().as_ref().is_some_and(|s| s.submit)) {
        event(stage);
    }
}
pub(crate) fn arm() {
    CURRENT.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            s.armed = true;
        }
    });
}
pub(crate) struct Transport {
    prior: Option<bool>,
    _thread: PhantomData<Rc<()>>,
}
pub(crate) fn transport(is_submit: bool) -> Transport {
    let prior = CURRENT.with(|s| {
        let mut s = s.borrow_mut();
        if let Some(s) = s.as_mut() {
            let old = s.submit;
            s.submit = s.armed && is_submit;
            Some(old)
        } else {
            None
        }
    });
    Transport {
        prior,
        _thread: PhantomData,
    }
}
impl Drop for Transport {
    fn drop(&mut self) {
        CURRENT.with(|s| {
            if let Some(s) = s.borrow_mut().as_mut()
                && let Some(prior) = self.prior
            {
                s.submit = prior;
            }
        });
    }
}
pub(crate) struct Http {
    end: Option<&'static str>,
    _thread: PhantomData<Rc<()>>,
}
pub(crate) fn http(method: &str, path: &str) -> Http {
    let enabled = CURRENT.with(|s| s.borrow().as_ref().is_some_and(|s| s.submit));
    let stages = if enabled {
        match (method, path) {
            ("GET", "/session") => Some(("session_http_send", "session_call_return")),
            ("GET", "/capabilities") => Some(("caps_http_send", "caps_call_return")),
            ("POST", "/entry-executor/control") => Some(("submit_http_send", "submit_call_return")),
            _ => None,
        }
    } else {
        None
    };
    if let Some((begin, _)) = stages {
        event(begin);
    }
    Http {
        end: stages.map(|(_, end)| end),
        _thread: PhantomData,
    }
}
impl Drop for Http {
    fn drop(&mut self) {
        if let Some(end) = self.end {
            event(end);
        }
    }
}
#[cfg(test)]
thread_local! { static OUTPUT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) }; }
#[cfg(test)]
pub(crate) fn test_scope() -> Scope {
    Scope::enter(true)
}
#[cfg(test)]
pub(crate) fn take_output() -> Vec<String> {
    OUTPUT.with(|o| std::mem::take(&mut *o.borrow_mut()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_disabled_and_nested_thread_scope_cleanup() {
        take_output();
        {
            let _off = Scope::enter(false);
            event("capture_return");
        }
        assert!(take_output().is_empty());
        {
            let _outer = test_scope();
            {
                let _nested = test_scope();
                event("capture_return");
            }
            std::thread::spawn(|| event("must_not_inherit"))
                .join()
                .unwrap();
        }
        assert_eq!(take_output().len(), 1);
        let _ = std::panic::catch_unwind(|| {
            let _s = test_scope();
            panic!("fixture");
        });
        assert_eq!(take_output().len(), 1);
        event("outside");
        assert!(take_output().is_empty());
    }
    #[test]
    fn nested_scopes_never_change_outer_events_or_transport_phase() {
        for nested_enabled in [true, false] {
            take_output();
            {
                let _outer = test_scope();
                event("capture_return");
                for submit in [false, true] {
                    let _transport = transport(submit);
                    {
                        let _nested = Scope::enter(nested_enabled);
                        event("inner_must_be_suppressed");
                        arm();
                        let _transport = transport(true);
                        let _http = http("POST", "/entry-executor/control");
                    }
                    let state = CURRENT.with(|s| {
                        let s = s.borrow();
                        let s = s.as_ref().unwrap();
                        (s.armed, s.submit, s.events.len())
                    });
                    assert_eq!(state, (false, false, 1));
                }
                arm();
                let _transport = transport(true);
                {
                    let _nested = Scope::enter(nested_enabled);
                    let _transport = transport(false);
                }
                {
                    let _http = http("GET", "/session");
                }
            }
            let output = take_output();
            assert_eq!(output.len(), 1);
            let value: serde_json::Value = serde_json::from_str(&output[0]).unwrap();
            let stages: Vec<_> = value["events"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e["stage"].as_str().unwrap())
                .collect();
            assert_eq!(
                stages,
                ["capture_return", "session_http_send", "session_call_return"]
            );
        }
    }
    #[test]
    fn bounded_fixed_fields_and_typed_submit_only() {
        take_output();
        {
            let _s = test_scope();
            {
                let _t = transport(true);
                let _h = http("POST", "/entry-executor/control");
            }
            arm();
            {
                let _t = transport(false);
                let _h = http("GET", "/session");
            }
            {
                let _t = transport(true);
                let _h = http("GET", "/session");
            }
            for _ in 0..40 {
                event("capture_return");
            }
        }
        let lines = take_output();
        let value: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(value["events"].as_array().unwrap().len(), 32);
        assert_eq!(value["truncated"], true);
        assert_eq!(value["events"][0]["stage"], "session_http_send");
        assert_eq!(value.as_object().unwrap().len(), 4);
        assert!(
            value["events"]
                .as_array()
                .unwrap()
                .windows(2)
                .all(|pair| pair[0]["elapsed_us"].as_u64().unwrap()
                    <= pair[1]["elapsed_us"].as_u64().unwrap())
        );
        for secret in [
            "entry-executor",
            "Cookie",
            "goal_text",
            "csrf",
            "http://",
            "must_not_inherit",
        ] {
            assert!(!lines[0].contains(secret));
        }
    }
}
