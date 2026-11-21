// OVERWRITE hook for Game::apply_mouse_look:
// - While flying with Elytra:
//   Mouse X rolls the aircraft (banking).
//   Mouse Y pitches in body coordinates (pull-up while banked carves).
//   (A/D rudder and the smoothed pitch/yaw are applied in the frame hook.)
// - While on ground / walking:
//   Standard vanilla mouse look (yaw & pitch).

fn on_mouse_look(game: &mut crate::game::Game<'_>) {
    let look = game.look_options();
    let time = game.started.elapsed().as_secs_f64();
    let pointer = game.captured || game.agent_pointer;
    let is_playing = match game.screen {
        crate::game::Screen::Playing => true,
        _ => false,
    };
    let grabbed = pointer
        && is_playing
        && (game.player.is_some() || game.studio.flying());
    let active = game.focused || game.agent_pointer;
    if let Some((xo, yo)) = game
        .mouse
        .handle_accumulated_movement(time, active, grabbed, look)
    {
        game.tutorial.mouse(xo, yo);
        let zoom = game.zoom_turn_scale();
        let xo = xo * zoom;
        let yo = yo * zoom;

        let gliding = match &game.player {
            Some(p) => p.fall_flying,
            None => false,
        };
        // Disable flight controls while underwater so the player can swim normally.
        let underwater = gliding
            && game.camera_submersion(game.view.eye) == horizon_client::fog::FogType::Water;

        if gliding && !underwater && crate::aerobatica::is_enabled() {
            let cur_roll = crate::aerobatica::get_roll();

            // Invert pitch is read in the frame hook (see frame_helper.rs) into
            // the runtime, which calculate_flight_turn applies before the roll decomposition.
            // Mouse Y decomposes through roll: pull-up while banked carves
            let (pitch_delta, roll_delta, carve_yaw) =
                crate::aerobatica::calculate_flight_turn(xo, yo, cur_roll, game.pitch);

            // Set target roll from mouse X input; tick() smooths toward it
            crate::aerobatica::add_target_roll(roll_delta);

            // Queue the pitch and carve yaw; the frame hook eases them into the
            // camera so they are as smooth as the roll. A/D rudder is handled there too.
            crate::aerobatica::add_pending_turn(pitch_delta, carve_yaw);
        } else {
            // Normal walking / sitting / riding controls
            game.turn_player(xo, yo);
        }
    }
}
