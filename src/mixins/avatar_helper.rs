// AFTER_CALL hook (after `mod_render_post`) in Game::player_model_geometry: banks the flying avatar with the camera.
// The game tips the avatar's body to the look pitch while it glides, but the camera roll
// never reached it, so the body stayed level while the view banked. The body, cape, elytra,
// held items and layers are all built by now, in `out` from where the HEAD hook noted, so this
// turns those vertices about the body's long axis by the camera's roll.
//
// It runs right after the mods' post hooks and before the glowing outline reads `out`, so the
// outline is drawn from the banked vertices too.
//
// The axis and pivot come from the avatar's own transform: the model's +y runs toward the
// feet, so the world direction of +y is the axis, and the model origin lies on it. Turning
// about +y by minus the roll puts the right side down for a positive (right) bank at every
// pitch, over the top of a loop included.
fn on_after(
    game: &crate::game::Game<'_>,
    out: &mut Vec<horizon_client_render::render_geometry::Vertex>,
    camera: glam::DVec3,
    alpha: f64,
) {
    let Some(m) = game.player_model.as_ref() else {
        return;
    };
    if !m.extra.fall_flying || m.extra.auto_spin_attack {
        return;
    }
    let roll = crate::aerobatica::get_roll();
    let start = crate::aerobatica::get_avatar_start();
    if roll.abs() < 0.001 || start >= out.len() {
        return;
    }
    // The glide pose eases in over ten ticks (the game's own tip-over scale); the roll follows it.
    let ticks = m.extra.fall_fly_ticks as f32 + alpha as f32;
    let blend = (ticks * ticks / 100.0).min(1.0);
    let pitch = m.previous_head_pitch + (m.head_pitch - m.previous_head_pitch) * alpha as f32;
    let (_, view_pitch) = crate::aerobatica::get_view();
    let degrees = crate::aerobatica::avatar_roll(roll, view_pitch, pitch, blend);

    let base = horizon_client::mobs::render::base_transform(m, alpha as f32, camera);
    let pivot = base.transform_point3(glam::Vec3::ZERO);
    let axis = base.transform_vector3(glam::Vec3::Y).normalize_or_zero();
    if axis == glam::Vec3::ZERO {
        return;
    }
    let turn = glam::Quat::from_axis_angle(axis, (-degrees).to_radians());
    for vertex in out[start..].iter_mut() {
        let at = glam::Vec3::from(vertex.pos);
        vertex.pos = (pivot + turn * (at - pivot)).to_array();
        let normal = glam::Vec3::new(
            f32::from(vertex.normal[0]),
            f32::from(vertex.normal[1]),
            f32::from(vertex.normal[2]),
        );
        if normal != glam::Vec3::ZERO {
            let turned = turn * normal;
            vertex.normal[0] = turned.x.round().clamp(-127.0, 127.0) as i8;
            vertex.normal[1] = turned.y.round().clamp(-127.0, 127.0) as i8;
            vertex.normal[2] = turned.z.round().clamp(-127.0, 127.0) as i8;
        }
    }
}
