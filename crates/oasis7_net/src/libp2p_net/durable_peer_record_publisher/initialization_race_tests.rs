use std::sync::mpsc;
use std::time::Duration;

use super::super::initialization::{INTENT_FILE_NAME, INTENT_TEMP_FILE_NAME, test_hooks};
use super::{DurablePeerRecordPublisher, Keypair, TestDirectory, sample_record};
use test_hooks::Stage;

// No sleeps or scheduler luck: the first initializer has passed its empty-path
// check before its competitor can publish and unlink the shared intent temp.
#[test]
fn stale_empty_check_cannot_poison_the_winning_initialization() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    let (empty_checked, wait_empty) = mpsc::channel();
    let (resume_first, wait_first) = mpsc::channel();
    let first_path = directory.0.clone();
    let first_key = keypair.clone();
    let first_template = template.clone();
    let first = std::thread::spawn(move || {
        test_hooks::with_hook(
            move |stage| {
                if stage == Stage::EmptyChecked {
                    empty_checked.send(()).expect("signal empty check");
                    wait_first
                        .recv_timeout(Duration::from_secs(10))
                        .expect("resume first");
                }
            },
            || DurablePeerRecordPublisher::initialize(&first_path, &first_key, &first_template),
        )
    });
    wait_empty
        .recv_timeout(Duration::from_secs(10))
        .expect("first empty check");

    let (second_progress, wait_second_progress) = mpsc::channel();
    let (resume_second, wait_second) = mpsc::channel();
    let second_path = directory.0.clone();
    let second_key = keypair.clone();
    let second_template = template.clone();
    let second = std::thread::spawn(move || {
        let published = second_progress.clone();
        let result = test_hooks::with_hook(
            move |stage| {
                if stage == Stage::IntentAcquired {
                    published.send(true).expect("signal second intent");
                    wait_second
                        .recv_timeout(Duration::from_secs(10))
                        .expect("resume second");
                }
            },
            || DurablePeerRecordPublisher::initialize(&second_path, &second_key, &second_template),
        );
        second_progress.send(false).expect("signal second exit");
        result
    });
    let second_published = wait_second_progress
        .recv_timeout(Duration::from_secs(10))
        .expect("second publishes or rejects concurrent initialization");
    resume_first.send(()).expect("resume first initializer");
    let first_result = first.join().expect("first initializer thread");
    if second_published {
        resume_second
            .send(())
            .expect("resume publishing initializer");
    }
    let second_result = second.join().expect("second initializer thread");
    let results = [first_result, second_result];
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "one initializer must complete without its competitor poisoning cleanup: {results:?}"
    );
    assert!(
        !directory.0.join(INTENT_FILE_NAME).exists(),
        "completed intent removed"
    );
    assert!(
        !directory.0.join(INTENT_TEMP_FILE_NAME).exists(),
        "no losing initializer temp"
    );
    DurablePeerRecordPublisher::open(&directory.0, &keypair, &template)
        .expect("ordinary open after the original concurrent attempt");
}
