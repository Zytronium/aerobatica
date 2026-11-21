// Barrel Roll runtime: elytra flight with aircraft-like controls.
//
// While gliding, mouse X rolls the aircraft (banking), mouse Y pitches it,
// and A/D keys act as rudder for yaw. The camera follows the aircraft
// attitude, so you fly like a pilot — rolling into turns with the mouse
// and coordinating with the rudder keys.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

/// The mod's row in Options > Mods. `version` here and in horizon-mixin.toml are kept the same.
pub const ID: &str = "gyro";
pub const VERSION: &str = "1.7.0";
pub const DESCRIPTION: &str =
    "Fly an elytra like an aircraft: mouse X rolls, mouse Y pitches, A/D keys steer (rudder). Smooth camera, invert Y option, and realistic flight physics.";

/// The game's own look sensitivity: 0.15 degrees per unit of mouse.
const DEG_PER_UNIT: f32 = 0.15;

/// How fast the world levels itself when you stop rolling, in degrees a second.
/// Slow enough that the bank is still there when you pull up to carve.
const LEVEL_RATE: f32 = 25.0;

/// Smoothing factor for camera roll: fraction of the distance to target covered
/// each frame. Higher values respond faster; lower values feel smoother.
const ROLL_SMOOTHING: f32 = 0.15;

/// How many degrees of yaw the rudder applies per second when A or D is held.
/// Matches the mouse turn rate so keyboard and mouse feel balanced.
const RUDDER_RATE: f32 = 90.0;

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

/// Whether mouse Y input for pitch is inverted.
static INVERT_Y: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
/// - `yo` pitches you, in body coordinates, so a banked pull-up carves.
/// - Returns `(pitch_delta, roll_delta, carve_yaw)`.
///
/// The roll is the same 0.15 degrees per unit the game's own look uses, so
/// banking feels exactly like looking.
pub fn calculate_flight_turn(xo: f64, yo: f64, current_roll: f32) -> (f32, f32, f32) {
    // Mouse X rolls the aircraft. This is the primary bank control.
    let roll_input = (xo as f32) * DEG_PER_UNIT;

    // Mouse Y is body pitch input, optionally inverted.
    let mut pitch_input = (yo as f32) * DEG_PER_UNIT;
    if get_invert_y() {
        pitch_input = -pitch_input;
    }

    let phi = current_roll.to_radians();
    let cos_phi = phi.cos();
    let sin_phi = phi.sin();

    // The body pitch projected onto world pitch and world yaw:
    // when level, a pull-up is a pure climb; when banked, the same pull-up
    // carves, because the lift comes off the tilted wing.
    let pitch_delta = pitch_input * cos_phi;
    let carve_yaw = -pitch_input * sin_phi;

    // Return pitch delta, roll delta, and the carve yaw component.
    // Total yaw = carve_yaw + rudder_yaw (from A/D keys).
    (pitch_delta, roll_input, carve_yaw)
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

/// Advance the flight state per frame: the roll smooths toward its target,
/// and levels out completely when you land.
/// Returns the current bank in degrees.
pub fn tick(gliding: bool, elapsed: Duration, pitch: f32) -> f32 {
    let dt = elapsed.as_secs_f32().min(0.5);

    let mut roll = get_roll();
    if gliding {
        // Update client-side physics (drag, lift, gravity)
        let _vel = update_physics(elapsed, pitch);

        // Smooth toward the target roll set by mouse input.
        let target = get_target_roll();
        let diff = target - roll;
        roll += diff * ROLL_SMOOTHING;

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
        let (pitch_delta, roll_delta, _) = calculate_flight_turn(100.0, 0.0, 0.0);
        assert!((roll_delta - 15.0).abs() < 1e-4, "mouse X must roll the aircraft");
        assert!(pitch_delta.abs() < 1e-4, "mouse X should not affect pitch");
    }

    #[test]
    fn test_mouse_x_rolls_the_aircraft_left() {
        let _guard = flight_state();
        let (_, roll_delta, _) = calculate_flight_turn(-100.0, 0.0, 0.0);
        assert!((roll_delta + 15.0).abs() < 1e-4, "mouse X must roll left");
    }

    #[test]
    fn test_banked_pitch_carves() {
        let _guard = flight_state();
        // Banked 90 deg right (phi = 90 deg):
        // Pulling up (yo = -100) carves yaw to the right
        let (p_delta, _, carve_yaw) = calculate_flight_turn(0.0, -100.0, 90.0);
        assert!(p_delta.abs() < 1e-3, "world pitch delta should be near 0");
        assert!(carve_yaw > 0.0, "should carve yaw right into the bank");
    }

    #[test]
    fn test_level_pitch_does_not_carve() {
        let _guard = flight_state();
        // A level pull-up is a pure climb: no carve yaw.
        set_invert_y(false);
        let (p_delta, _, carve_yaw) = calculate_flight_turn(0.0, -100.0, 0.0);
        assert!(p_delta < 0.0, "a level pull-up should pitch up");
        assert!(carve_yaw.abs() < 1e-3, "a level pull-up must not carve");
    }

    #[test]
    fn test_inverted_pitch_flips_direction() {
        let _guard = flight_state();
        // Without invert: yo = -100 gives negative pitch (up).
        set_invert_y(false);
        let (p_delta, _, _) = calculate_flight_turn(0.0, -100.0, 0.0);
        assert!(p_delta < 0.0, "normal pitch should be negative for yo < 0");

        // With invert: yo = -100 gives positive pitch (down).
        set_invert_y(true);
        let (p_delta, _, _) = calculate_flight_turn(0.0, -100.0, 0.0);
        assert!(p_delta > 0.0, "inverted pitch should flip direction");
        set_invert_y(false);
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
    fn test_roll_levels_out_on_ground() {
        let _guard = flight_state();
        set_roll(90.0);
        set_target_roll(0.0);
        set_velocity(0.0);
        let roll = tick(false, Duration::from_secs_f32(0.5), 0.0);
        assert!(roll < 90.0, "roll should return toward 0 when on the ground");
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
