// Barrel Roll runtime: elytra flight with aircraft-like controls.
//
// While gliding, mouse X rolls the aircraft (banking), mouse Y pitches it,
// and A/D keys act as rudder for yaw. The camera follows the aircraft
// attitude, so you fly like a pilot — rolling into turns with the mouse
// and coordinating with the rudder keys.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

/// The mod's row in Options > Mods. `version` here and in horizon-mod.toml are kept the same.
pub const ID: &str = "aerobatica";
pub const VERSION: &str = "1.0.3";
pub const DESCRIPTION: &str =
    "Fly an elytra like an aircraft: mouse X rolls, mouse Y pitches, A/D keys steer (rudder). Smooth camera, invert Y option, and realistic flight physics.";

/// The game's own look sensitivity: 0.15 degrees per unit of mouse.
const DEG_PER_UNIT: f32 = 0.15;

/// How fast the world levels itself when you stop rolling, in degrees a second.
/// Slow enough that the bank is still there when you pull up to carve.
const LEVEL_RATE: f32 = 10.0;

/// Time constant, in seconds, of the roll and yaw easing at the default camera
/// smoothness. At 60 fps that covers 15% of the remaining distance a frame, and
/// it stays the same at any frame rate.
const BASE_SMOOTHING_TAU: f32 = 0.1026;

/// Camera smoothness setting (0 to 10) that gives `BASE_SMOOTHING_TAU`. The tau
/// scales linearly with the setting; 0 is instant, 5 is the default, 10 is very smooth.
const DEFAULT_SMOOTHNESS: f32 = 5.0;
const MAX_SMOOTHNESS: f32 = 10.0;

/// Pitch eases this many times slower than roll and yaw, so a pull-up or push-over
/// builds up gently instead of snapping.
const PITCH_SMOOTHING_FACTOR: f32 = 2.0;

/// How often, in seconds, the mod's settings files are re-read.
const SETTINGS_REFRESH: f32 = 0.25;


/// How many degrees of yaw the rudder applies per second when A or D is held.
/// Matches the mouse turn rate so keyboard and mouse feel balanced.
const RUDDER_RATE: f32 = 90.0;

/// How many degrees of automatic yaw per second at full bank (90°).
/// Scales linearly with the roll angle so a shallow bank turns gently.
/// This is pure world yaw (about the vertical axis), not decomposed through
/// the aircraft's roll axis — unlike the rudder, it never pitches the nose.
const ROLL_YAW_RATE: f32 = 30.0;

/// How fast the world levels itself when you land, in degrees a second.
const GROUND_LEVEL_RATE: f32 = 270.0;

// --- Client-side flight physics ---

/// Terminal glide speed in meters per second (approximate elytra speed).
const TERMINAL_VELOCITY: f32 = 40.0;

/// Drag coefficient: how quickly velocity decays toward terminal speed per second.
const DRAG_COEFFICIENT: f32 = 0.05;

/// Gravity acceleration in meters per second squared.
const GRAVITY: f32 = 9.8;

/// Current estimated forward velocity in m/s (client-side prediction only).
static VELOCITY: AtomicU32 = AtomicU32::new(0);

/// Current camera bank, in degrees. Signed: positive is banked right.
static ROLL_DEGREES: AtomicU32 = AtomicU32::new(0);

/// Target camera bank the mouse input wants to reach, in degrees.
static TARGET_ROLL: AtomicU32 = AtomicU32::new(0);


/// The heading the last frame saw, so the turn can be measured per frame.
static LAST_YAW: AtomicU32 = AtomicU32::new(0);

/// The render camera's yaw in degrees, captured each frame from the view
/// the level is drawn with (third-person flips included).
pub static VIEW_YAW: AtomicU32 = AtomicU32::new(0);

/// The render camera's pitch in degrees.
pub static VIEW_PITCH: AtomicU32 = AtomicU32::new(0);

/// Entity id of the camera's mob: only that avatar locks to the roll.
pub static LOCK_MOB: AtomicU32 = AtomicU32::new(u32::MAX);

/// Where the avatar's own vertices begin in the frame's entity list, noted when
/// its drawing starts so the roll can be applied to just those after it is built.
static AVATAR_START: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Pitch (degrees) the mouse and rudder have asked for that is not applied yet.
static PENDING_PITCH: AtomicU32 = AtomicU32::new(0);

/// Yaw (degrees) the mouse and rudder have asked for that is not applied yet.
static PENDING_YAW: AtomicU32 = AtomicU32::new(0);

/// Camera smoothness setting, 0 (instant) to 10 (very smooth), bit-cast f32.
static SMOOTHNESS: AtomicU32 = AtomicU32::new(0x40A0_0000); // 5.0

/// Seconds since the settings were last read from disk, bit-cast f32. Starts
/// past the refresh period so the first frame reads them.
static SETTINGS_AGE: AtomicU32 = AtomicU32::new(0x4120_0000); // 10.0

/// Whether mouse Y input for pitch is inverted.
static INVERT_Y: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the last tick was gliding, so we can detect the takeoff transition.
static WAS_GLIDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the mod is currently enabled (toggle keybind).
static IS_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Whether the toggle key was pressed last frame (for edge detection in the mixin).
static TOGGLE_KEY_LAST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Check whether the mod is enabled.
pub fn get_enabled() -> bool {
    IS_ENABLED.load(Ordering::Relaxed)
}

/// Set whether the mod is enabled.
pub fn set_enabled(enabled: bool) {
    IS_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Get the current camera bank in degrees.
pub fn get_roll() -> f32 {
    f32::from_bits(ROLL_DEGREES.load(Ordering::Relaxed))
}

/// Set the current camera bank in degrees.
pub fn set_roll(deg: f32) {
    ROLL_DEGREES.store(deg.to_bits(), Ordering::Relaxed);
}

/// Turn the camera bank by `deg` degrees.
pub fn add_roll(deg: f32) {
    set_roll(get_roll() + deg);
}

/// Get the target camera bank the mouse input wants to reach.
pub fn get_target_roll() -> f32 {
    f32::from_bits(TARGET_ROLL.load(Ordering::Relaxed))
}

/// Set the target camera bank. The `tick()` function smooths toward this value.
pub fn set_target_roll(deg: f32) {
    TARGET_ROLL.store(deg.to_bits(), Ordering::Relaxed);
}

/// Add to the target camera bank. The `tick()` function smooths toward the new target.
pub fn add_target_roll(deg: f32) {
    set_target_roll(get_target_roll() + deg);
}

/// Flip the camera bank over, for the top of a loop.
pub fn flip_roll() {
    set_roll(get_roll() + 180.0);
}


/// The heading the last frame saw.
pub fn get_last_yaw() -> f32 {
    f32::from_bits(LAST_YAW.load(Ordering::Relaxed))
}

/// Record the heading this frame saw.
pub fn set_last_yaw(yaw: f32) {
    LAST_YAW.store(yaw.to_bits(), Ordering::Relaxed);
}

/// Record the camera orientation the frame renders with.
pub fn set_view(yaw: f32, pitch: f32) {
    VIEW_YAW.store(yaw.to_bits(), Ordering::Relaxed);
    VIEW_PITCH.store(pitch.to_bits(), Ordering::Relaxed);
}

/// The camera orientation recorded by `set_view`.
pub fn get_view() -> (f32, f32) {
    (
        f32::from_bits(VIEW_YAW.load(Ordering::Relaxed)),
        f32::from_bits(VIEW_PITCH.load(Ordering::Relaxed)),
    )
}

/// Record which entity the camera belongs to (`u32::MAX`: none).
pub fn set_lock_mob(id: u32) {
    LOCK_MOB.store(id, Ordering::Relaxed);
}

/// The entity id whose avatar locks to the camera roll.
pub fn get_lock_mob() -> u32 {
    LOCK_MOB.load(Ordering::Relaxed)
}

/// Check whether pitch input is inverted.
pub fn get_invert_y() -> bool {
    INVERT_Y.load(Ordering::Relaxed)
}

/// Set whether pitch input is inverted.
pub fn set_invert_y(invert: bool) {
    INVERT_Y.store(invert, Ordering::Relaxed);
}

/// The camera smoothness setting, 0 (instant) to 10 (very smooth).
pub fn get_smoothness() -> f32 {
    f32::from_bits(SMOOTHNESS.load(Ordering::Relaxed))
}

/// Set the camera smoothness, clamped to 0..=10.
pub fn set_smoothness(value: f32) {
    let value = if value.is_finite() { value.clamp(0.0, MAX_SMOOTHNESS) } else { DEFAULT_SMOOTHNESS };
    SMOOTHNESS.store(value.to_bits(), Ordering::Relaxed);
}

/// Fraction of the remaining distance an easing with the given time constant
/// covers in `dt` seconds. A zero constant is instant.
fn ease_fraction(dt: f32, tau: f32) -> f32 {
    if tau <= 1e-4 {
        1.0
    } else {
        1.0 - (-dt / tau).exp()
    }
}

/// Time constant of the roll and yaw easing at the current smoothness.
/// Multiplied by 3 so the smoothness setting has a more noticeable effect.
fn smoothing_tau() -> f32 {
    BASE_SMOOTHING_TAU * (get_smoothness() / DEFAULT_SMOOTHNESS) * 3.0
}

/// Whether it is time to re-read the settings files. Call once a frame.
pub fn settings_refresh_due(elapsed: Duration) -> bool {
    let age = f32::from_bits(SETTINGS_AGE.load(Ordering::Relaxed)) + elapsed.as_secs_f32();
    if age >= SETTINGS_REFRESH {
        SETTINGS_AGE.store(0.0f32.to_bits(), Ordering::Relaxed);
        true
    } else {
        SETTINGS_AGE.store(age.to_bits(), Ordering::Relaxed);
        false
    }
}

/// Apply the raw text of the settings files (`None`: never changed, so the
/// default). The game keeps a toggle as `true`/`false` and a range as a number.
///
/// Mixin code runs outside the mod's own call scope, so the typed reads
/// (`setting_bool` and friends) find no mod and return nothing there; the raw
/// read by mod id works anywhere, and this parses it.
pub fn apply_raw_settings(invert_y: Option<&str>, smoothness: Option<&str>) {
    set_invert_y(invert_y.map(str::trim) == Some("true"));
    let smooth = smoothness
        .and_then(|text| text.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite() && (0.0..=MAX_SMOOTHNESS).contains(v))
        .unwrap_or(DEFAULT_SMOOTHNESS);
    set_smoothness(smooth);
}

/// Whether the mod should process flight input this frame.
pub fn is_enabled() -> bool {
    get_enabled()
}

/// Check the toggle key (I key) with rising-edge detection.
/// Call once per frame from the mixin. `is_pressed` is the raw key state.
/// Returns true if the key was just pressed this frame.
pub fn check_toggle_key(is_pressed: bool) -> bool {
    let last = TOGGLE_KEY_LAST.load(Ordering::Relaxed);
    TOGGLE_KEY_LAST.store(is_pressed, Ordering::Relaxed);
    is_pressed && !last
}

/// Toggle the mod on/off. Returns the new state.
pub fn toggle_enabled() -> bool {
    let current = IS_ENABLED.load(Ordering::Relaxed);
    IS_ENABLED.store(!current, Ordering::Relaxed);
    !current
}

/// Queue a pitch and yaw change (degrees). It is applied gradually by
/// `take_smoothed_turn`, the same way the roll eases toward its target.
pub fn add_pending_turn(pitch: f32, yaw: f32) {
    PENDING_PITCH.store(
        (f32::from_bits(PENDING_PITCH.load(Ordering::Relaxed)) + pitch).to_bits(),
        Ordering::Relaxed,
    );
    PENDING_YAW.store(
        (f32::from_bits(PENDING_YAW.load(Ordering::Relaxed)) + yaw).to_bits(),
        Ordering::Relaxed,
    );
}

/// Drop any queued pitch and yaw (on landing, so it does not fire later).
pub fn clear_pending_turn() {
    PENDING_PITCH.store(0.0f32.to_bits(), Ordering::Relaxed);
    PENDING_YAW.store(0.0f32.to_bits(), Ordering::Relaxed);
}

/// Take this frame's share of the queued pitch and yaw. Returns
/// `(pitch_delta, yaw_delta)` in degrees. The share depends on the frame time,
/// so the easing feels the same at any frame rate.
pub fn take_smoothed_turn(elapsed: Duration) -> (f32, f32) {
    let dt = elapsed.as_secs_f32().min(0.5);
    let tau = smoothing_tau();
    let frac_yaw = ease_fraction(dt, tau);
    let frac_pitch = ease_fraction(dt, tau * PITCH_SMOOTHING_FACTOR);
    let pending_pitch = f32::from_bits(PENDING_PITCH.load(Ordering::Relaxed));
    let pending_yaw = f32::from_bits(PENDING_YAW.load(Ordering::Relaxed));
    let mut dp = pending_pitch * frac_pitch;
    let mut dy = pending_yaw * frac_yaw;
    // Flush the last crumbs so the easing ends instead of tailing off forever.
    if (pending_pitch - dp).abs() < 0.001 {
        dp = pending_pitch;
    }
    if (pending_yaw - dy).abs() < 0.001 {
        dy = pending_yaw;
    }
    PENDING_PITCH.store((pending_pitch - dp).to_bits(), Ordering::Relaxed);
    PENDING_YAW.store((pending_yaw - dy).to_bits(), Ordering::Relaxed);
    (dp, dy)
}

/// Get the estimated forward velocity in m/s.
pub fn get_velocity() -> f32 {
    f32::from_bits(VELOCITY.load(Ordering::Relaxed))
}

/// Set the estimated forward velocity in m/s.
pub fn set_velocity(v: f32) {
    VELOCITY.store(v.to_bits(), Ordering::Relaxed);
}

/// Update flight physics: apply drag, lift, and gravity to the estimated velocity.
/// Returns the updated velocity.
/// - Drag slows the aircraft toward terminal glide speed.
/// - Lift from pitch angle affects vertical motion.
/// - Gravity pulls the aircraft down when lift is insufficient.
pub fn update_physics(elapsed: Duration, pitch: f32) -> f32 {
    let dt = elapsed.as_secs_f32().min(0.5);
    let mut vel = get_velocity();

    // Initialize velocity to terminal speed if it hasn't been set yet
    if vel == 0.0 {
        vel = TERMINAL_VELOCITY;
    }

    // Drag: reduce velocity toward terminal glide speed
    if vel > TERMINAL_VELOCITY {
        vel -= DRAG_COEFFICIENT * (vel - TERMINAL_VELOCITY) * dt * 60.0;
    } else if vel < TERMINAL_VELOCITY {
        // Natural glide maintains speed; slight drag below terminal
        vel -= DRAG_COEFFICIENT * 0.5 * (vel - TERMINAL_VELOCITY) * dt * 60.0;
    }

    // Gravity and lift based on pitch angle
    let pitch_rad = pitch.to_radians();
    // Negative pitch (looking down) increases speed; positive pitch (looking up) decreases it
    let pitch_effect = -pitch_rad.sin() * GRAVITY * dt;
    vel += pitch_effect;

    // Clamp to reasonable bounds
    vel = vel.clamp(5.0, 80.0);

    set_velocity(vel);
    vel
}

/// Transform mouse movements `(xo, yo)` into flight updates:
/// - `xo` rolls the aircraft (banking), at the game's own look sensitivity.
/// - `yo` pitches in body coordinates, so a pull-up while banked carves.
/// - Returns `(pitch_delta, roll_delta, carve_yaw)`.
///
/// The roll is the same 0.15 degrees per unit the game's own look uses, so
/// banking feels exactly like looking. The pitch is decomposed through the
/// current roll angle so the lift vector follows the wing — pulling up while
/// banked 90° carves horizontally instead of climbing. The invert Y setting
/// negates the pitch input before decomposition, so it works at all roll angles.
///
/// `current_pitch` is the camera pitch in degrees. Once a loop carries it past
/// straight up or down (|pitch| in 90..270 degrees) the view is drawn upside
/// down relative to the roll, so the screen's "up" is the opposite of what the
/// roll alone says. The carve direction is flipped there to match what is seen.
pub fn calculate_flight_turn(
    xo: f64,
    yo: f64,
    current_roll: f32,
    current_pitch: f32,
) -> (f32, f32, f32) {
    // Mouse X rolls the aircraft. This is the primary bank control.
    let roll_input = (xo as f32) * DEG_PER_UNIT;

    // Mouse Y is body pitch input, optionally inverted.
    // Apply invert Y BEFORE decomposition so the setting works at all roll angles.
    let mut pitch_input = (yo as f32) * DEG_PER_UNIT;
    if get_invert_y() {
        pitch_input = -pitch_input;
    }

    // Decompose the body pitch onto world pitch and world yaw based on roll angle.
    // When level (roll=0), a pull-up is a pure climb (full pitch, no carve).
    // When banked 90°, the same pull-up carves yaw to the right (no pitch, full carve).
    // This is how real aircraft work: the lift vector tilts with the wing.
    let phi = current_roll.to_radians();
    let cos_phi = phi.cos();
    let sin_phi = phi.sin();

    // Past the vertical (half-loop), the camera is on the far side of the pole:
    // yaw is mirrored and the picture is upside down. The effective bank the
    // pilot sees is roll + 180, which negates both sin and cos. The pitch
    // delta already works out the same because pitch itself runs backwards
    // there, so only the carve needs the flip.
    let over_the_top = current_pitch.to_radians().cos() < 0.0;
    let side = if over_the_top { -1.0 } else { 1.0 };

    let pitch_delta = pitch_input * cos_phi;
    let carve_yaw = -pitch_input * sin_phi * side;

    // Return pitch delta, roll delta, and the carve yaw component.
    // Total yaw = carve_yaw + rudder_yaw (from A/D keys).
    (pitch_delta, roll_input, carve_yaw)
}

/// Calculate automatic yaw from the current roll angle, like a real airplane.
/// Returns degrees of pure world yaw for this frame: negative for left bank,
/// positive for right bank. The rate scales linearly with the sine of the roll
/// angle so level flight produces no yaw and full bank produces the maximum.
///
/// Unlike rudder yaw (which rotates about the aircraft's vertical axis and can
/// pitch the nose when banked), this is always pure world yaw — the heading
/// changes relative to zero roll, never tilting toward or away from the ground.
///
/// `pitch` is the camera pitch in degrees. After a half-loop carries it past
/// straight up or down (|pitch| in 90..270 degrees) the view is drawn upside
/// down relative to the roll, so the bank the pilot sees is roll + 180 and the
/// turn must mirror, exactly as in `calculate_flight_turn` and `decompose_rudder`.
pub fn calculate_roll_yaw(roll: f32, pitch: f32, elapsed: Duration) -> f32 {
    let dt = elapsed.as_secs_f32().min(0.5);
    let roll_rad = roll.to_radians();
    let side = if pitch.to_radians().cos() < 0.0 { -1.0 } else { 1.0 };
    ROLL_YAW_RATE * roll_rad.sin() * side * dt
}

/// Note where the avatar's vertices begin (the list's length before it is drawn).
pub fn set_avatar_start(index: usize) {
    AVATAR_START.store(index, Ordering::Relaxed);
}

/// Where the avatar's vertices begin in the list noted by `set_avatar_start`.
pub fn get_avatar_start() -> usize {
    AVATAR_START.load(Ordering::Relaxed)
}

/// The roll, in degrees, to give the flying avatar's body about its long axis
/// (positive banks it right), so it banks with the camera as it pitches with it.
///
/// `view_pitch` is the camera's pitch and `avatar_pitch` the pitch the avatar is
/// drawn with, both in degrees. Normally they agree and the avatar takes the
/// camera's roll. If one of them is past the vertical of a loop and the other is
/// not (the game wrapped or mirrored the avatar's pitch), the two bodies are
/// 180 degrees apart about the flight path, so the roll gains a half turn.
/// `blend` is 0..1, how far the glide pose has eased in.
pub fn avatar_roll(roll: f32, view_pitch: f32, avatar_pitch: f32, blend: f32) -> f32 {
    let mirrored = view_pitch.to_radians().cos() * avatar_pitch.to_radians().cos() < 0.0;
    let roll = if mirrored { roll + 180.0 } else { roll };
    roll * blend
}

/// Calculate yaw delta from rudder keys (A/D).
/// Returns degrees of yaw per frame: negative for left (A), positive for right (D).
/// The rate matches the mouse turn sensitivity so keyboard and mouse feel balanced.
pub fn calculate_rudder_yaw(key_left: bool, key_right: bool, elapsed: Duration) -> f32 {
    let dt = elapsed.as_secs_f32().min(0.5);
    let mut yaw = 0.0f32;
    if key_left {
        yaw -= RUDDER_RATE;
    }
    if key_right {
        yaw += RUDDER_RATE;
    }
    yaw * dt
}

/// Turn a rudder yaw (degrees, positive is right) into `(pitch_delta, yaw_delta)`
/// for the camera, about the aircraft's own vertical axis instead of the world's.
///
/// Level, the rudder is pure yaw. Banked 90 degrees right, the aircraft's
/// right-hand side points at the ground, so right rudder pushes the nose down.
/// Past the vertical of a loop the view is drawn upside down, so the yaw is
/// mirrored, exactly as in `calculate_flight_turn`.
pub fn decompose_rudder(rudder_yaw: f32, current_roll: f32, current_pitch: f32) -> (f32, f32) {
    let phi = current_roll.to_radians();
    let side = if current_pitch.to_radians().cos() < 0.0 { -1.0 } else { 1.0 };
    (rudder_yaw * phi.sin(), rudder_yaw * phi.cos() * side)
}

/// The same view as `(yaw, pitch, roll)`, with the pitch brought back within straight
/// down and straight up. Returns `(yaw, pitch, roll, changed)`.
///
/// A half-loop carries the pitch past vertical. The picture is the same as looking
/// the other way round and upside down: `(yaw, pitch, roll)` draws exactly as
/// `(yaw + 180, 180 - pitch, roll + 180)`. While gliding the raw values are fine, but
/// everything else (swimming, walking, the server) reads the yaw and a pitch within
/// +-90 degrees, so past vertical it faces the opposite way to the camera and the
/// avatar. Called when the glide ends, so the player really faces where the camera
/// looks and nothing visibly jumps.
pub fn upright_attitude(yaw: f32, pitch: f32, roll: f32) -> (f32, f32, f32, bool) {
    let wrapped = (pitch + 180.0).rem_euclid(360.0) - 180.0;
    if wrapped.to_radians().cos() >= -1e-5 {
        return (yaw, wrapped, roll, (wrapped - pitch).abs() > 1e-4);
    }
    let flipped = if wrapped > 0.0 { 180.0 - wrapped } else { -180.0 - wrapped };
    let turned_roll = (roll + 180.0 + 180.0).rem_euclid(360.0) - 180.0;
    (yaw + 180.0, flipped, turned_roll, true)
}

/// Advance the flight state per frame: the roll smooths toward its target,
/// and levels out completely when you land.
/// Returns the current bank in degrees.
pub fn tick(gliding: bool, elapsed: Duration, pitch: f32) -> f32 {
    let dt = elapsed.as_secs_f32().min(0.5);
    let was_gliding = WAS_GLIDING.load(Ordering::Relaxed);
    WAS_GLIDING.store(gliding, Ordering::Relaxed);

    let mut roll = get_roll();
    if gliding {
        // Just took off: reset roll state so the camera doesn't spin to catch up
        // to a stale target roll accumulated while on the ground.
        if !was_gliding {
            roll = 0.0;
            set_target_roll(0.0);
        }

        // Update client-side physics (drag, lift, gravity)
        let _vel = update_physics(elapsed, pitch);

        // Smooth toward the target roll set by mouse input.
        let target = get_target_roll();
        let diff = target - roll;
        roll += diff * ease_fraction(dt, smoothing_tau());

        // If the target is near zero and we're close, also apply the natural
        // leveling to settle cleanly.
        if target.abs() < 0.01 {
            let step = LEVEL_RATE * dt;
            if roll.abs() <= step {
                roll = 0.0;
            } else {
                roll -= roll.signum() * step;
            }
        }

    } else {
        // On the ground, reset velocity and level out quickly.
        set_velocity(0.0);
        clear_pending_turn();
        // Normalize roll to [-180, 180] so we take the shortest path to 0.
        // Without this, a roll of 720° would spin multiple full circles instead
        // of just unwinding to the nearest 0.
        roll = (roll + 180.0).rem_euclid(360.0) - 180.0;
        let step = GROUND_LEVEL_RATE * dt;
        if roll.abs() <= step {
            roll = 0.0;
        } else {
            roll -= roll.signum() * step;
        }
    }
    set_roll(roll);
    roll
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// The flight state lives in statics, and the tests run in parallel, so
    /// they take this lock and run one at a time.
    fn flight_state() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn test_mouse_x_rolls_the_aircraft() {
        let _guard = flight_state();
        // Moving mouse right rolls the aircraft right (positive roll).
        let (pitch_delta, roll_delta, _) = calculate_flight_turn(100.0, 0.0, 0.0, 0.0);
        assert!((roll_delta - 15.0).abs() < 1e-4, "mouse X must roll the aircraft");
        assert!(pitch_delta.abs() < 1e-4, "mouse X should not affect pitch");
    }

    #[test]
    fn test_mouse_x_rolls_the_aircraft_left() {
        let _guard = flight_state();
        let (_, roll_delta, _) = calculate_flight_turn(-100.0, 0.0, 0.0, 0.0);
        assert!((roll_delta + 15.0).abs() < 1e-4, "mouse X must roll left");
    }

    #[test]
    fn test_level_pitch_is_pure_climb() {
        let _guard = flight_state();
        set_invert_y(false);
        // At 0° bank, a pull-up is a pure climb: no carve yaw.
        let (p_delta, _, carve_yaw) = calculate_flight_turn(0.0, -100.0, 0.0, 0.0);
        assert!(p_delta < 0.0, "a level pull-up should pitch up");
        assert!((p_delta - (-15.0)).abs() < 1e-4, "pitch delta should be -15 degrees");
        assert!(carve_yaw.abs() < 1e-4, "a level pull-up must not carve");
    }

    #[test]
    fn test_banked_pitch_carves() {
        let _guard = flight_state();
        set_invert_y(false);
        // Banked 90° right: pull-up carves yaw to the right, no world pitch.
        let (p_delta, _, carve_yaw) = calculate_flight_turn(0.0, -100.0, 90.0, 0.0);
        assert!(p_delta.abs() < 1e-3, "world pitch should be near 0 at 90° bank, got {}", p_delta);
        assert!(carve_yaw > 0.0, "should carve yaw right into the bank");
        assert!((carve_yaw - 15.0).abs() < 1e-3, "carve yaw should be ~15 degrees, got {}", carve_yaw);
    }

    #[test]
    fn test_pitch_direction_at_negative_90_roll() {
        let _guard = flight_state();
        set_invert_y(false);
        // At ±90° roll, cos(±90°) ≈ 0, so world pitch is near 0.
        // The carve yaw should have opposite signs at +90° vs -90°.
        let (_, _, carve_90) = calculate_flight_turn(0.0, -100.0, 90.0, 0.0);
        let (_, _, carve_neg90) = calculate_flight_turn(0.0, -100.0, -90.0, 0.0);
        assert!(carve_90 > 0.0, "carve at +90° should be positive");
        assert!(carve_neg90 < 0.0, "carve at -90° should be negative");
    }

    #[test]
    fn test_pitch_direction_at_180_roll() {
        let _guard = flight_state();
        set_invert_y(false);
        // At 180° roll (upside down), cos(180°) = -1, so pitch is inverted.
        // A pull-up (yo < 0, pitch_input < 0) gives pitch_delta = pitch_input * cos(180°) = positive.
        // This means when upside down, pulling up pitches the nose down in world space,
        // which is correct aircraft behavior.
        let (p_delta, _, _) = calculate_flight_turn(0.0, -100.0, 180.0, 0.0);
        assert!(p_delta > 0.0, "pitch at 180° roll should be positive (inverted), got {}", p_delta);
        assert!((p_delta - 15.0).abs() < 1e-3, "pitch should be full strength at 180°, got {}", p_delta);
    }

    #[test]
    fn test_invert_y_at_all_roll_angles() {
        let _guard = flight_state();
        // Invert Y should flip the pitch input before decomposition,
        // so the sign of pitch_delta is always flipped regardless of roll angle.
        for roll in [0.0_f32, 45.0, 135.0, 180.0, -45.0, -135.0, -180.0] {
            set_invert_y(false);
            let (p_normal, _, _) = calculate_flight_turn(0.0, -100.0, roll, 0.0);

            set_invert_y(true);
            let (p_inverted, _, _) = calculate_flight_turn(0.0, -100.0, roll, 0.0);

            // Skip angles where cos(roll) ≈ 0 (near 90°), pitch is near 0
            if p_normal.abs() > 0.01 {
                assert!(
                    (p_normal < 0.0 && p_inverted > 0.0) || (p_normal > 0.0 && p_inverted < 0.0),
                    "invert Y should flip pitch direction at {}° roll: normal={}, inverted={}",
                    roll, p_normal, p_inverted
                );
            }
        }
        set_invert_y(false);
    }

    #[test]
    fn test_inverted_pitch_flips_direction() {
        let _guard = flight_state();
        // Without invert: yo = -100 gives negative pitch (up).
        set_invert_y(false);
        let (p_delta, _, _) = calculate_flight_turn(0.0, -100.0, 0.0, 0.0);
        assert!(p_delta < 0.0, "normal pitch should be negative for yo < 0");

        // With invert: yo = -100 gives positive pitch (down).
        set_invert_y(true);
        let (p_delta, _, _) = calculate_flight_turn(0.0, -100.0, 0.0, 0.0);
        assert!(p_delta > 0.0, "inverted pitch should flip direction");
        set_invert_y(false);
    }

    #[test]
    fn test_carve_flips_past_vertical() {
        let _guard = flight_state();
        set_invert_y(false);
        // Half-loop from inverted: roll 180, pitch swept past 90. Rolled left
        // (-30 over the top reads as 150), a pull-up must go up AND left.
        // Upright, rolled left: pull-up carves left (negative yaw).
        let (_, _, carve_upright) = calculate_flight_turn(0.0, -100.0, -30.0, 0.0);
        assert!(carve_upright < 0.0, "upright left bank carves left");
        // Over the top the picture is upside down, so the same bank value
        // (roll + 180 as displayed) must carve the mirrored way.
        let (p_delta, _, carve_over) = calculate_flight_turn(0.0, -100.0, 150.0, 170.0);
        // Effective bank there is 150 + 180 = -30 (left): pull-up carves left.
        assert!(carve_over < 0.0, "over-the-top left bank must carve left, got {}", carve_over);
        // And world pitch moves the camera toward screen-up: pitch grows
        // (170 -> 180) because pitch runs backwards past vertical.
        assert!(p_delta > 0.0, "pitch should keep sweeping through the loop, got {}", p_delta);
    }

    #[test]
    fn test_roll_yaw_mirrors_past_vertical() {
        let dt = Duration::from_secs_f32(1.0 / 60.0);
        // Upright, banked right: turns right.
        assert!(calculate_roll_yaw(45.0, 0.0, dt) > 0.0);
        // After a half roll and half loop the pitch is past vertical and the
        // picture is upside down again: the same bank value turns the other way.
        assert!(calculate_roll_yaw(45.0, 170.0, dt) < 0.0);
        assert!(calculate_roll_yaw(45.0, -170.0, dt) < 0.0);
        // Back through a full loop (pitch 360) it is the original way again.
        assert!(calculate_roll_yaw(45.0, 360.0, dt) > 0.0);
    }

    #[test]
    fn test_avatar_takes_the_camera_roll() {
        assert_eq!(avatar_roll(30.0, 10.0, 10.0, 1.0), 30.0);
        assert_eq!(avatar_roll(-30.0, 120.0, 120.0, 1.0), -30.0, "both past vertical agree");
        assert_eq!(avatar_roll(30.0, 10.0, 10.0, 0.5), 15.0, "eases in with the pose");
    }

    #[test]
    fn test_avatar_roll_gains_a_half_turn_when_pitch_is_mirrored() {
        // Camera past vertical (half-loop), avatar drawn with the wrapped pitch.
        assert_eq!(avatar_roll(30.0, 120.0, 60.0, 1.0), 210.0);
        assert_eq!(avatar_roll(30.0, 60.0, 120.0, 1.0), 210.0);
        assert_eq!(avatar_roll(30.0, -120.0, -60.0, 1.0), 210.0);
    }

    #[test]
    fn test_upright_attitude_after_half_roll_and_half_loop() {
        // Inverted, then looped to level: the camera looks back and is upright.
        let (yaw, pitch, roll, changed) = upright_attitude(30.0, 180.0, 180.0);
        assert!(changed);
        assert!((yaw - 210.0).abs() < 1e-3 && pitch.abs() < 1e-3 && roll.abs() < 1e-3,
                "got {} {} {}", yaw, pitch, roll);
        // Part way over the top.
        let (yaw, pitch, roll, _) = upright_attitude(0.0, 170.0, 30.0);
        assert!((yaw - 180.0).abs() < 1e-3 && (pitch - 10.0).abs() < 1e-3, "got {} {}", yaw, pitch);
        assert!((roll + 150.0).abs() < 1e-3, "roll gains a half turn, got {}", roll);
        let (_, pitch, _, _) = upright_attitude(0.0, -170.0, 0.0);
        assert!((pitch + 10.0).abs() < 1e-3);
    }

    #[test]
    fn test_upright_attitude_leaves_a_normal_view_alone() {
        assert_eq!(upright_attitude(30.0, 45.0, 10.0), (30.0, 45.0, 10.0, false));
        assert_eq!(upright_attitude(30.0, -90.0, 0.0), (30.0, -90.0, 0.0, false));
        // A pitch wrapped a full turn is the same view, brought back in range.
        let (yaw, pitch, _, changed) = upright_attitude(30.0, 390.0, 0.0);
        assert!(changed && yaw == 30.0 && (pitch - 30.0).abs() < 1e-3);
    }

    #[test]
    fn test_upright_attitude_is_the_same_view() {
        // The view is the matrix Rz(roll) * Rx(pitch) * Ry(yaw), as the camera draws it.
        fn view(yaw: f32, pitch: f32, roll: f32) -> [[f32; 3]; 3] {
            let (y, p, r) = (yaw.to_radians(), pitch.to_radians(), roll.to_radians());
            let ry = [[y.cos(), 0.0, y.sin()], [0.0, 1.0, 0.0], [-y.sin(), 0.0, y.cos()]];
            let rx = [[1.0, 0.0, 0.0], [0.0, p.cos(), -p.sin()], [0.0, p.sin(), p.cos()]];
            let rz = [[r.cos(), -r.sin(), 0.0], [r.sin(), r.cos(), 0.0], [0.0, 0.0, 1.0]];
            let mul = |a: [[f32; 3]; 3], b: [[f32; 3]; 3]| {
                let mut o = [[0.0; 3]; 3];
                for i in 0..3 { for j in 0..3 { for k in 0..3 { o[i][j] += a[i][k] * b[k][j]; } } }
                o
            };
            mul(rz, mul(rx, ry))
        }
        for &(yaw, pitch, roll) in &[(30.0, 180.0, 180.0), (-80.0, 130.0, 25.0), (10.0, -140.0, -60.0)] {
            let (y2, p2, r2, _) = upright_attitude(yaw, pitch, roll);
            let (a, b) = (view(yaw, pitch, roll), view(y2, p2, r2));
            for i in 0..3 { for j in 0..3 {
                assert!((a[i][j] - b[i][j]).abs() < 1e-4, "({yaw},{pitch},{roll}) -> ({y2},{p2},{r2})");
            } }
        }
    }

    #[test]
    fn test_rudder_is_pure_yaw_when_level() {
        let (p, y) = decompose_rudder(10.0, 0.0, 0.0);
        assert!(p.abs() < 1e-4 && (y - 10.0).abs() < 1e-4, "got {} {}", p, y);
    }

    #[test]
    fn test_rudder_pitches_down_when_banked_right() {
        let (p, y) = decompose_rudder(10.0, 90.0, 0.0);
        assert!((p - 10.0).abs() < 1e-3, "right rudder at 90 right bank pushes the nose down, got {}", p);
        assert!(y.abs() < 1e-3);
        let (p, _) = decompose_rudder(10.0, -90.0, 0.0);
        assert!(p < -9.9, "banked left it pushes the nose up, got {}", p);
    }

    #[test]
    fn test_rudder_yaw_mirrors_past_vertical() {
        let (_, y) = decompose_rudder(10.0, 0.0, 170.0);
        assert!(y < 0.0, "yaw mirrors over the top, got {}", y);
    }

    #[test]
    fn test_pitch_eases_slower_than_yaw() {
        let _guard = flight_state();
        set_smoothness(DEFAULT_SMOOTHNESS);
        clear_pending_turn();
        add_pending_turn(10.0, 10.0);
        let (p, y) = take_smoothed_turn(Duration::from_secs_f32(1.0 / 60.0));
        assert!(p > 0.0 && p < y, "pitch should lag yaw, got pitch {} yaw {}", p, y);
    }

    #[test]
    fn test_smoothness_zero_is_instant_and_max_is_slower() {
        let _guard = flight_state();
        let dt = Duration::from_secs_f32(1.0 / 60.0);
        set_smoothness(0.0);
        clear_pending_turn();
        add_pending_turn(10.0, 10.0);
        let (p, y) = take_smoothed_turn(dt);
        assert!((p - 10.0).abs() < 1e-3 && (y - 10.0).abs() < 1e-3, "got {} {}", p, y);
        set_smoothness(2.5);
        clear_pending_turn();
        add_pending_turn(10.0, 0.0);
        let (mid, _) = take_smoothed_turn(dt);
        set_smoothness(10.0);
        clear_pending_turn();
        add_pending_turn(10.0, 0.0);
        let (high, _) = take_smoothed_turn(dt);
        assert!(high < mid, "more smoothness must move less per frame: {} vs {}", high, mid);
        set_smoothness(DEFAULT_SMOOTHNESS);
        clear_pending_turn();
    }

    #[test]
    fn test_raw_settings_parse() {
        let _guard = flight_state();
        apply_raw_settings(Some("true\n"), Some("3.0"));
        assert!(get_invert_y());
        assert!((get_smoothness() - 3.0).abs() < 1e-6);
        apply_raw_settings(Some("false"), Some("15"));
        assert!(!get_invert_y());
        assert_eq!(get_smoothness(), DEFAULT_SMOOTHNESS, "out of range reads as the default");
        apply_raw_settings(None, None);
        assert!(!get_invert_y());
        assert_eq!(get_smoothness(), DEFAULT_SMOOTHNESS);
    }

    #[test]
    fn test_settings_refresh_is_throttled() {
        let _guard = flight_state();
        SETTINGS_AGE.store(10.0f32.to_bits(), Ordering::Relaxed);
        assert!(settings_refresh_due(Duration::from_millis(16)), "first frame reads");
        assert!(!settings_refresh_due(Duration::from_millis(16)));
        assert!(settings_refresh_due(Duration::from_millis(300)));
    }

    #[test]
    fn test_turn_is_smoothed_and_completes() {
        let _guard = flight_state();
        set_smoothness(DEFAULT_SMOOTHNESS);
        clear_pending_turn();
        add_pending_turn(10.0, -20.0);
        let (p1, y1) = take_smoothed_turn(Duration::from_secs_f32(1.0 / 60.0));
        assert!(p1 > 0.0 && p1 < 10.0, "first frame covers only part of the turn, got {}", p1);
        assert!(y1 < 0.0 && y1 > -20.0);
        let (mut tp, mut ty) = (p1, y1);
        for _ in 0..600 {
            let (p, y) = take_smoothed_turn(Duration::from_secs_f32(1.0 / 60.0));
            tp += p;
            ty += y;
        }
        assert!((tp - 10.0).abs() < 1e-2 && (ty + 20.0).abs() < 1e-2, "got {} {}", tp, ty);
    }

    #[test]
    fn test_turn_smoothing_is_frame_rate_independent() {
        let _guard = flight_state();
        set_smoothness(DEFAULT_SMOOTHNESS);
        let total = |fps: f32| {
            clear_pending_turn();
            add_pending_turn(10.0, 0.0);
            let mut t = 0.0;
            for _ in 0..(fps * 0.2) as usize {
                t += take_smoothed_turn(Duration::from_secs_f32(1.0 / fps)).0;
            }
            t
        };
        assert!((total(30.0) - total(240.0)).abs() < 0.1);
    }

    #[test]
    fn test_rudder_yaws_left() {
        let _guard = flight_state();
        // Holding A (key_left) should yaw left (negative).
        // dt is clamped to 0.5s max, so use 0.5s to get the full rate.
        let yaw = calculate_rudder_yaw(true, false, Duration::from_secs_f32(0.5));
        assert!(yaw < 0.0, "A key should yaw left");
        assert!(
            (yaw - (-45.0)).abs() < 0.1,
            "rudder rate should be RUDDER_RATE * 0.5s = -45, got {}",
            yaw
        );
    }

    #[test]
    fn test_rudder_yaws_right() {
        let _guard = flight_state();
        // Holding D (key_right) should yaw right (positive).
        let yaw = calculate_rudder_yaw(false, true, Duration::from_secs_f32(0.5));
        assert!(yaw > 0.0, "D key should yaw right");
        assert!(
            (yaw - 45.0).abs() < 0.1,
            "rudder rate should be RUDDER_RATE * 0.5s = 45, got {}",
            yaw
        );
    }

    #[test]
    fn test_rudder_no_input() {
        let _guard = flight_state();
        let yaw = calculate_rudder_yaw(false, false, Duration::from_secs_f32(0.5));
        assert!(yaw.abs() < 1e-4, "no rudder input should give zero yaw");
    }

    #[test]
    fn test_roll_yaw_is_zero_when_level() {
        let _guard = flight_state();
        // No roll means no automatic yaw.
        let yaw = calculate_roll_yaw(0.0, 0.0, Duration::from_secs_f32(1.0 / 60.0));
        assert!(yaw.abs() < 1e-6, "level flight should produce no roll yaw, got {}", yaw);
    }

    #[test]
    fn test_roll_yaw_is_positive_at_right_bank() {
        let _guard = flight_state();
        // Banked right (positive roll) should yaw right (positive yaw).
        let yaw = calculate_roll_yaw(45.0, 0.0, Duration::from_secs_f32(1.0 / 60.0));
        assert!(yaw > 0.0, "right bank should yaw right, got {}", yaw);
    }

    #[test]
    fn test_roll_yaw_is_negative_at_left_bank() {
        let _guard = flight_state();
        // Banked left (negative roll) should yaw left (negative yaw).
        let yaw = calculate_roll_yaw(-45.0, 0.0, Duration::from_secs_f32(1.0 / 60.0));
        assert!(yaw < 0.0, "left bank should yaw left, got {}", yaw);
    }

    #[test]
    fn test_roll_yaw_max_at_90_bank() {
        let _guard = flight_state();
        // At 90° bank, sin(90°) = 1, so yaw rate is maximum.
        // dt is clamped to 0.5s in the function, so use 0.5s for the expected value.
        let yaw = calculate_roll_yaw(90.0, 0.0, Duration::from_secs_f32(0.5));
        let expected = ROLL_YAW_RATE * 90.0_f32.to_radians().sin() * 0.5;
        assert!(
            (yaw - expected).abs() < 1e-4,
            "90° bank should give max roll yaw rate, got {} expected {}",
            yaw, expected
        );
    }

    #[test]
    fn test_roll_yaw_is_pure_yaw() {
        let _guard = flight_state();
        // Unlike rudder yaw, roll-induced yaw should never produce a pitch component.
        // It is always applied as pure world yaw (pitch=0, yaw=roll_yaw).
        // This test verifies the function only returns a yaw value, not a (pitch, yaw) pair.
        let yaw = calculate_roll_yaw(60.0, 0.0, Duration::from_secs_f32(1.0 / 60.0));
        // The yaw should be non-zero for a non-trivial bank.
        assert!(yaw.abs() > 1e-6, "60° bank should produce yaw, got {}", yaw);
        // Compare with rudder at same bank: rudder would produce pitch + yaw,
        // but roll_yaw is just a single f32 (pure yaw).
        let (rudder_pitch, rudder_yaw) = decompose_rudder(30.0, 60.0, 0.0);
        assert!(rudder_pitch.abs() > 1e-3, "rudder at 60° bank should produce pitch");
        assert!(
            (rudder_yaw - 30.0 * 60.0_f32.to_radians().cos()).abs() < 1e-3,
            "rudder yaw is decomposed through roll axis"
        );
        // Roll yaw is independent of pitch decomposition — it's always pure world yaw.
    }

    #[test]
    fn test_roll_yaw_scales_with_dt() {
        let _guard = flight_state();
        // Yaw should scale linearly with frame time.
        let yaw_1 = calculate_roll_yaw(45.0, 0.0, Duration::from_secs_f32(1.0 / 60.0));
        let yaw_2 = calculate_roll_yaw(45.0, 0.0, Duration::from_secs_f32(1.0 / 30.0));
        assert!(
            (yaw_2 - yaw_1 * 2.0).abs() < 1e-5,
            "doubling dt should double yaw, got {} vs {}",
            yaw_2, yaw_1 * 2.0
        );
    }

    #[test]
    fn test_roll_levels_when_gliding() {
        let _guard = flight_state();
        set_roll(45.0);
        set_target_roll(0.0);
        set_velocity(0.0);
        // Target is 0, roll should level toward it.
        let roll = tick(true, Duration::from_secs_f32(0.5), 0.0);
        assert!(roll < 45.0, "roll should level when gliding without input");
        assert!(roll >= 0.0, "it should not level past level");
    }

    #[test]
    fn test_roll_smooths_toward_target() {
        let _guard = flight_state();
        set_roll(0.0);
        set_target_roll(45.0);
        set_velocity(0.0);
        // Mark as already gliding so the takeoff reset doesn't interfere.
        WAS_GLIDING.store(true, Ordering::Relaxed);
        // Roll should move toward the target but not reach it in one frame.
        let roll = tick(true, Duration::from_secs_f32(0.05), 0.0);
        assert!(roll > 0.0, "roll should move toward target");
        assert!(roll < 45.0, "roll should not overshoot the target");
    }

    #[test]
    fn test_roll_converges_to_target() {
        let _guard = flight_state();
        set_roll(0.0);
        set_target_roll(45.0);
        set_velocity(0.0);
        // Mark as already gliding so the takeoff reset doesn't interfere.
        WAS_GLIDING.store(true, Ordering::Relaxed);
        // After many frames, roll should converge close to the target.
        for _ in 0..50 {
            let _ = tick(true, Duration::from_secs_f32(0.05), 0.0);
        }
        let roll = get_roll();
        assert!(
            (roll - 45.0).abs() < 1.0,
            "roll should converge to target, got {}",
            roll
        );
    }

    #[test]
    fn test_takeoff_resets_roll() {
        let _guard = flight_state();
        // Simulate accumulated roll from previous flight (e.g., 720° = 2 full rolls).
        set_roll(720.0);
        set_target_roll(100.0);
        set_velocity(0.0);
        // Mark as on the ground before takeoff.
        WAS_GLIDING.store(false, Ordering::Relaxed);
        // First tick while gliding should reset roll to 0, not spin to 720°.
        let roll = tick(true, Duration::from_secs_f32(0.05), 0.0);
        assert!(
            roll.abs() < 1.0,
            "takeoff should reset roll to 0°, got {}",
            roll
        );
        assert!(
            get_target_roll().abs() < 0.01,
            "takeoff should reset target roll to 0°"
        );
    }

    #[test]
    fn test_roll_levels_out_on_ground() {
        let _guard = flight_state();
        set_roll(90.0);
        set_target_roll(0.0);
        set_velocity(0.0);
        let roll = tick(false, Duration::from_secs_f32(0.5), 0.0);
        assert!(roll < 90.0, "roll should return toward 0 when on the ground");
    }

    #[test]
    fn test_roll_overshoot_normalized_on_ground() {
        let _guard = flight_state();
        // Simulate multiple full rolls accumulated (e.g., 720° = 2 full rolls).
        // When landing, it should normalize to the shortest path and not spin.
        set_roll(720.0);
        set_target_roll(0.0);
        set_velocity(0.0);
        // 720° normalizes to 0° (720 % 360 = 0), so it should snap to 0 immediately.
        let roll = tick(false, Duration::from_secs_f32(0.5), 0.0);
        assert!(
            roll.abs() < 1.0,
            "720° roll should normalize to 0° on ground, got {}",
            roll
        );
        // 719° normalizes to -1° (719 % 360 = 359, 359 - 360 = -1)
        set_roll(719.0);
        let roll = tick(false, Duration::from_secs_f32(0.5), 0.0);
        assert!(
            roll.abs() < 2.0,
            "719° roll should normalize to ~0° on ground, got {}",
            roll
        );
        // -1080° normalizes to 0°
        set_roll(-1080.0);
        let roll = tick(false, Duration::from_secs_f32(0.5), 0.0);
        assert!(
            roll.abs() < 1.0,
            "-1080° roll should normalize to 0° on ground, got {}",
            roll
        );
    }

    #[test]
    fn test_physics_velocity_initializes() {
        let _guard = flight_state();
        set_velocity(0.0);
        // First physics update should initialize to terminal velocity.
        let vel = update_physics(Duration::from_secs_f32(0.05), 0.0);
        assert!(
            (vel - TERMINAL_VELOCITY).abs() < 1.0,
            "velocity should initialize to terminal speed, got {}",
            vel
        );
    }

    #[test]
    fn test_physics_pitch_down_increases_speed() {
        let _guard = flight_state();
        set_velocity(TERMINAL_VELOCITY);
        // Pitching down (negative pitch) should increase speed.
        let vel = update_physics(Duration::from_secs_f32(0.5), -30.0);
        assert!(
            vel > TERMINAL_VELOCITY,
            "pitching down should increase speed, got {}",
            vel
        );
    }

    #[test]
    fn test_physics_pitch_up_decreases_speed() {
        let _guard = flight_state();
        set_velocity(TERMINAL_VELOCITY);
        // Pitching up (positive pitch) should decrease speed.
        let vel = update_physics(Duration::from_secs_f32(0.5), 30.0);
        assert!(
            vel < TERMINAL_VELOCITY,
            "pitching up should decrease speed, got {}",
            vel
        );
    }

    #[test]
    fn test_physics_velocity_clamped() {
        let _guard = flight_state();
        set_velocity(100.0);
        // Velocity should be clamped to max 80.
        let vel = update_physics(Duration::from_secs_f32(0.05), 0.0);
        assert!(vel <= 80.0, "velocity should be clamped to 80, got {}", vel);
    }

    #[test]
    fn test_view_round_trip() {
        set_view(-135.25, 41.75);
        let (yaw, pitch) = get_view();
        assert_eq!((yaw, pitch), (-135.25, 41.75));
    }

    #[test]
    fn test_lock_mob_round_trip() {
        set_lock_mob(7);
        assert_eq!(get_lock_mob(), 7);
        set_lock_mob(u32::MAX);
        assert_eq!(get_lock_mob(), u32::MAX);
    }
}
