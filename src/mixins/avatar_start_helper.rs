// HEAD hook for Game::player_model_geometry: notes where our own avatar's vertices
// begin in the entity list, so the avatar_helper AFTER_CALL hook can bank just those.
fn on_head(out: &Vec<horizon_client_render::render_geometry::Vertex>) {
    crate::gyroflight::set_avatar_start(out.len());
}
