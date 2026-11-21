// Gyroflight — aircraft-like elytra flight controls for Horizon.
//
// Mouse X rolls, mouse Y pitches, A/D keys steer (rudder).
// Smooth camera, invert Y option, and realistic flight physics.
//
// The runtime module contains all flight logic and is injected into the game
// as a support module via [[native.module]] in horizon-mod.toml.
//
// The mod uses the native route ([[native.mixin]]) for all game hooks.
// A minimal Hooks impl is required by the launcher to provide the entry point
// and register settings (invert Y toggle, camera smoothness).

pub mod gyroflight;

#[mod_api::horizon_mod]
impl mod_api::Hooks for Hooks {
    fn register(&self, r: &mut dyn mod_api::Registrar, _: &mod_api::RegistrationContext) {
        // Register the invert Y axis setting — appears in Options > Mods > Gyroflight > Config
        let _ = r.register_setting(mod_api::SettingSpec::toggle(
            "invert_y",
            "Invert pitch",
            false,
        ));
        // Camera smoothness during flight: 0 is instant, 10 is very smooth.
        // Affects roll, pitch and yaw easing; pitch is always eased a bit more.
        let _ = r.register_setting(mod_api::SettingSpec::range(
            "camera_smoothness",
            "Camera smoothness",
            0.0,
            10.0,
            0.25,
            5.0,
        ));
    }
}

#[derive(Default)]
struct Hooks;
