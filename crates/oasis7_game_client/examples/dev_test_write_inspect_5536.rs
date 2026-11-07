//! Trusted read-only inspection before selecting a bounded move target.
use std::io::Read;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("executor config required")?;
    let mut unlock = zeroize::Zeroizing::new(Vec::new());
    std::io::stdin().take(1025).read_to_end(&mut unlock)?;
    if unlock.len() > 1024 {
        return Err("unlock too large".into());
    }
    let client = oasis7_game_client::device_client::Client::open_authenticated(
        std::path::Path::new(&path),
        &unlock,
    )?;
    let view = client.public_view()?;
    println!(
        "{}",
        serde_json::json!({"agent":client.agent,"world":client.world,"tick":view.logical_tick(),"state":view.projection().state,"agent_context":view.projection().agent_context,"capabilities":client.capabilities()?})
    );
    Ok(())
}
