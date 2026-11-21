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
    // Disable flight physics while underwater so the player can swim normally.
    let underwater = gliding
        && game.camera_submersion(game.view.eye) == horizon_client::fog::FogType::Water;
    if !gliding || underwater {
        // A half-loop leaves the pitch past vertical, which draws the same as looking the
        // other way round and upside down. The glide is over, so make the player really
        // face where the camera looks (swimming and walking read yaw and a pitch within 90).
        let (yaw, pitch, roll, changed) =
            crate::gyroflight::upright_attitude(game.yaw, game.pitch, crate::gyroflight::get_roll());
        if changed {
            let turned = yaw - game.yaw;
            game.yaw = yaw;
            game.pitch = pitch;
            crate::gyroflight::set_roll(roll);
            crate::gyroflight::set_target_roll(0.0);
            // The avatar's own copy follows, so it does not spin to catch up.
            if let Some(m) = game.player_model.as_mut() {
                m.head_pitch = pitch;
                m.previous_head_pitch = pitch;
                m.head_yaw += turned;
                m.previous_head_yaw += turned;
                m.body_yaw += turned;
                m.previous_body_yaw += turned;
            }
        }
    }
    let _ = crate::gyroflight::tick(gliding && !underwater, elapsed, game.pitch);

    if gliding && !underwater {
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

        // Automatic yaw from roll angle: like a real airplane, banking naturally
        // turns the heading. This is pure world yaw (about the vertical axis),
        // not decomposed through the aircraft's roll axis — so it never pitches
        // the nose toward or away from the ground.
        let roll = crate::gyroflight::get_roll();
        let roll_yaw = crate::gyroflight::calculate_roll_yaw(roll, game.pitch, elapsed);
        if roll_yaw.abs() > 1e-6 {
            crate::gyroflight::add_pending_turn(0.0, roll_yaw);
        }

        // Ease the queued turn into the camera, like the roll does.
        let (dp, dy) = crate::gyroflight::take_smoothed_turn(elapsed);
        game.pitch += dp;
        game.yaw += dy;
    }
}