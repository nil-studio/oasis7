//! Auxiliary exact worker; only the parent acceptance case can count a pass.
use super::*;
#[test]
#[ignore = "auxiliary fresh writer process; invoked only by the full-node acceptance owner"]
fn formal_full_node_cold_writer_worker() {
    let path = std::env::var_os(INPUT_ENV).expect("cold worker input is required");
    thread::Builder::new()
        .name("fresh-cold-writer".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || run(Path::new(&path)))
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
}
fn run(path: &Path) -> Result<(), String> {
    let input: Input = read(path)?;
    // SAFETY: getppid has no caller preconditions; no parent data is accessed.
    let actual_parent = unsafe { libc::getppid() } as u32;
    if input.schema != 1
        || input.parent_pid != actual_parent
        || input.parent_pid == std::process::id()
        || hex::decode(&input.nonce).map_err(|e| e.to_string())?.len() != 32
        || input.remaining_ms == 0
        || input.remaining_ms > 21_600_000
        || ![10, 1000, 10000].contains(&input.requested)
    {
        return Err("cold worker input version/PID/nonce/budget mismatch".into());
    }
    let directory = path.parent().ok_or("cold worker input parent missing")?;
    crate::controlled_live_config::require_private_owned(directory, true)?;
    let mut budget = Budget {
        started: Instant::now(),
        maximum: Duration::from_millis(input.remaining_ms),
        progress: None,
        complete: true,
        requested: input.requested,
        completed_commits: input.requested,
        // Physical directory inventory is owned by the parent fixture.
        inventory_entries: 5_000_000,
    };
    // A fresh address space opens persisted config; no parent World/Arc or
    // process-global restore/proof cache can be present in this worker.
    let mut runtime = FormalLiveRuntime::open_with_native_history(&input.runtime_config)?;
    let current = runtime.published.current()?;
    let original_key =
        correlation::key_digest(&input.original.correlation.key).map_err(|e| e.to_string())?;
    let mut receipt = Receipt {
        schema: 2,
        nonce: input.nonce.clone(),
        parent_pid: actual_parent,
        child_pid: std::process::id(),
        stage: "ready".into(),
        fixed_head: io::digest(&current.anchor.head().head())?,
        authority: io::digest(current.anchor.authority().definition())?,
        typed: io::typed_history(&current.world, &mut budget)?,
        result: io::digest(&retained_result_from_hot_or_cold(
            &current.world,
            &original_key,
        )?)?,
        effect: io::digest(
            &game_task::current_control(&current.world, "agent").map_err(|e| e.to_string())?,
        )?,
        emits: wasm_emit_count(&current.world),
        original_commit: None,
        continuation_commit: None,
        final_position: current.anchor.head().record().position,
        continuation_completed: false,
        ready_node_running: runtime
            .runtime
            .lock()
            .map_err(|_| "cold runtime lock poisoned")?
            .snapshot()
            .running,
    };
    if receipt.fixed_head != input.fixed_head
        || receipt.authority != input.authority
        || receipt.typed != input.typed
        || receipt.result != input.result
        || receipt.effect != input.effect
        || receipt.emits != input.emits
        || receipt.final_position != input.fixed_position
        || receipt.ready_node_running
    {
        return Err("fresh writer actual fixed-head/typed/original read differs".into());
    }
    let fixed_position = receipt.final_position;
    drop(current);
    publish(&directory.join("ready.json"), &receipt)?;
    // Node has never started. Parent alone performs quiescent directory
    // accounting and P/R restarts before authorizing the first new Tick.
    let go = directory.join("continue.json");
    loop {
        if budget.started.elapsed() >= budget.maximum {
            return Err("cold worker continue deadline exhausted".into());
        }
        if go.try_exists().map_err(|e| e.to_string())? {
            let token: (u32, String, u32, u32) = read(&go)?;
            if token != (1, input.nonce.clone(), actual_parent, std::process::id()) {
                return Err("cold writer continue token mismatch".into());
            }
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    runtime.start()?;
    let owner_config = input.owner.restore()?;
    let owner = RemoteWorldServiceClient::new(owner_config.clone()).map_err(|e| e.to_string())?;
    if lookup_original_commit(&owner, &input.original)? != input.original_commit {
        return Err("cold original CommitRef changed".into());
    }
    let retry = retry_original_at_exact_publication(&input.original, &runtime, &owner, &budget)?;
    assert_admin_commit_did_not_emit_wasm(
        &runtime.validated.config.writer_directory,
        &retry.commit,
    )?;
    let after = retry.after;
    let commit_matches = lookup_original_commit(&owner, &input.original)? == input.original_commit;
    let result_matches = io::digest(&retained_result_from_hot_or_cold(
        &after.world,
        &original_key,
    )?)? == input.result;
    let effect_matches =
        io::digest(&game_task::current_control(&after.world, "agent").map_err(|e| e.to_string())?)?
            == input.effect;
    let retry_emits = wasm_emit_count(&after.world);
    assert_retry_emission_count(input.emits, retry.adjacent_before_emits, retry_emits)?;
    if !commit_matches || !result_matches || !effect_matches {
        return Err(format!(
            "fresh cold original retry changed original result or effect: \
             commit_matches={commit_matches}, result_matches={result_matches}, \
             effect_matches={effect_matches}, emits_expected={}, emits_observed={retry_emits}",
            input.emits
        ));
    }
    drop(after);
    let key: [u8; 32] = hex::decode(&input.owner_key)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "cold owner key length")?;
    let continuation = submit_change_owned(
        &owner_config,
        &input.identity,
        &SigningKey::from_bytes(&key),
        &runtime,
        input.requested,
    )?;
    let continuation_commit = lookup_original_commit(&owner, &continuation)?;
    if continuation_commit.position <= fixed_position {
        return Err("cold writer continuation did not advance the fixed head".into());
    }
    // Background ticks may publish separately; inspect the exact delegation
    // publication to detect any emissions caused by administrative execution.
    assert_admin_commit_did_not_emit_wasm(
        &runtime.validated.config.writer_directory,
        &continuation_commit,
    )?;
    let duplicate_commit = lookup_original_commit(&owner, &input.original)?;
    stop_owned_protected_runtime(
        &mut runtime,
        budget
            .phase_deadline(Instant::now())?
            .saturating_duration_since(Instant::now()),
        "fresh_full_node_final_stop",
    )?;
    let final_cut = runtime.published.current()?;
    receipt.result = io::digest(&retained_result_from_hot_or_cold(
        &final_cut.world,
        &original_key,
    )?)?;
    receipt.effect = io::digest(
        &game_task::current_control(&final_cut.world, "agent").map_err(|e| e.to_string())?,
    )?;
    receipt.emits = wasm_emit_count(&final_cut.world);
    receipt.final_position = final_cut.anchor.head().record().position;
    let result_matches = receipt.result == input.result;
    let effect_matches = receipt.effect == input.effect;
    if !result_matches || !effect_matches || receipt.final_position < continuation_commit.position {
        return Err(format!(
            "fresh writer continuation changed original unique effect/result: \
             result_matches={result_matches}, effect_matches={effect_matches}, \
             emits_expected={}, emits_observed={}, fixed_position={fixed_position}, final_position={}",
            input.emits, receipt.emits, receipt.final_position
        ));
    }
    receipt.original_commit = Some(duplicate_commit);
    if receipt.original_commit.as_ref() != Some(&input.original_commit) {
        return Err("fresh final original CommitRef changed".into());
    }
    receipt.continuation_commit = Some(continuation_commit);
    receipt.stage = "done".into();
    receipt.continuation_completed = true;
    drop(final_cut);
    drop(runtime);
    publish(&directory.join("done.json"), &receipt)
}
