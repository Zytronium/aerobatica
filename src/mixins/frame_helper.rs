// Frame hook: the roll levels itself when you stop moving the mouse,
// flight physics (drag, lift, gravity) are updated, and the queued pitch/yaw
// from the mouse and the A/D rudder is eased into the camera.
fn on_frame(game: &mut crate::game::Game<'_>, elapsed: std::time::Duration) {
    // The mod's settings (invert pitch, camera smoothness). Mixin code runs outside
    // the mod's call scope, where the typed reads (`setting_bool` etc.) find no mod
    // and return None, so read the raw files by mod id, a few times a second.
    if crate::gyroflight::settings_refresh_due(elapsed) {
        let invert = game.setting("gyroflight", "invert_y");
        let smooth = game.setting("gyroflight", "camera_smoothness");
        crate::gyroflight::apply_raw_settings(invert.as_deref(), smooth.as_deref());
    }

    let gliding = match &game.player {
        Some(p) => p.fall_flying,
        None => false,
    };
    let _ = crate::gyroflight::tick(gliding, elapsed, game.pitch);

    if gliding {
        let is_playing = match game.screen {
            crate::game::Screen::Playing => true,
            _ => false,
        };
        let controlling = (game.captured || game.agent_pointer) && is_playing;
        if controlling {
            // A/D rudder turns about the aircraft's own vertical axis, so it
            // follows the current roll and pitch rather than being pure world yaw.
            let key_a = game.keys.raw(&winit::keyboard::KeyCode::KeyA);
            let key_d = game.keys.raw(&winit::keyboard::KeyCode::KeyD);
            let rudder = crate::gyroflight::calculate_rudder_yaw(key_a, key_d, elapsed);
            if rudder != 0.0 {
                let (p, y) = crate::gyroflight::decompose_rudder(
                    rudder,
                    crate::gyroflight::get_roll(),
                    game.pitch,
                );
                crate::gyroflight::add_pending_turn(p, y);
            }
        }

        // Ease the queued turn into the camera, like the roll does.
        let (dp, dy) = crate::gyroflight::take_smoothed_turn(elapsed);
        game.pitch += dp;
        game.yaw += dy;
    }
}
