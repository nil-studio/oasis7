use std::cell::RefCell;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::libp2p_net) enum Stage {
    EmptyChecked,
    IntentAcquired,
}

type Hook = Box<dyn FnMut(Stage)>;
thread_local! {
    static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
}

pub(in crate::libp2p_net) fn reach(stage: Stage) {
    HOOK.with_borrow_mut(|hook| {
        if let Some(hook) = hook {
            hook(stage);
        }
    });
}

pub(in crate::libp2p_net) fn with_hook<T>(
    hook: impl FnMut(Stage) + 'static,
    run: impl FnOnce() -> T,
) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            HOOK.with_borrow_mut(|hook| *hook = None);
        }
    }
    HOOK.with_borrow_mut(|current| {
        assert!(current.is_none(), "initialization test hook must not nest");
        *current = Some(Box::new(hook));
    });
    let _reset = Reset;
    run()
}
