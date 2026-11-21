// Frame hook: the roll levels itself when you stop moving the mouse,
// and flight physics (drag, lift, gravity) are updated.
fn on_frame(game: &mut crate::game::Game<'_>, elapsed: std::time::Duration) {
    let gliding = match &game.player {
        Some(p) => p.fall_flying,
        None => false,
    };
    let _ = horizon_client::gyro::tick(gliding, elapsed, game.pitch);
}
