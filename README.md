# Aerobatica

Aerobatica turns elytra gliding into aircraft style flight on Horizon. Move the mouse left and right to
bank the wings, move it up and down to pitch, and press A or D for rudder controlled yaw. Pull up while
banked and the aircraft carves a turn. Push the nose past vertical and you complete a loop. The camera
eases smoothly into every movement and levels out when you touch down.

Inspired by the Java Edition mod *Do a Barrel Roll*. Built as a separate project from the original Gyro
mod by [CGRLabs](https://github.com/CGRLabs).

## Credits

Aerobatica is a fork of [Gyro](https://github.com/CGRLabs/gyro) by [CGRLabs](https://github.com/CGRLabs).
The original mod introduced camera roll and smooth banking to Horizon elytra flight. Aerobatica extends
that foundation with Do a Barrel Roll style controls, rudder input, invert pitch settings, and refined
flight physics.

## Controls

| While gliding | What it does |
|---|---|
| Mouse left / right | **Roll**: banks the aircraft left or right |
| Mouse up / down | Pitch: climb and dive; through the bank, a pull-up carves |
| A key | **Yaw left** (rudder) |
| D key | **Yaw right** (rudder) |
| Anything else | Vanilla controls for walking, flying and swimming |

**The bank is the barrel roll.** Keep rolling in one direction and the world keeps leaning over,
further and further, until you have rolled right around and come back to level. A full barrel roll,
done with nothing but the mouse.

No key bindings to configure. Open **Options > Mods > Aerobatica > Config** for two settings: **Invert pitch**
and **Camera smoothness** (0 is instant, 1 is very smooth). Roll, pitch and yaw all ease toward their
target, with pitch smoothing slightly more than the others.

## Play it

**Through the Horizon launcher** (the usual way for a native mod):

```sh
horizon-launcher native-build . --instance "My Instance" --trust-native-code
```

The launcher downloads its own Rust toolchain and compiles the mod against the prebuilt game. The first
build takes 20 to 30 minutes, while later builds are incremental. It requests hook points v3, so it
builds on any Horizon version that supports them.

**Important:** The launcher only accepts specific files in the mod directory. Before building, move any
extra files such as `rust-toolchain.toml`, `tools/`, and `target/` out of the way. The launcher rejects
non standard paths like `mixins/`.

Native mods run as unsandboxed code with your system permissions. This mod is not reviewed by the Horizon
registry, so the launcher will warn you in red before building. The source consists of a few `.rs` files
under `src/`. Review them before trusting the build.

Once the mod is built, launch the game and test it: `/give @s minecraft:elytra 1`, jump off something
high, and hold Space to glide.

## Server compatibility

Aerobatica only moves the camera and the player model on the client. The server never receives roll data,
so the mod is safe on strict servers. However, rudder input and carved turns do change your movement
direction, so an anticheat that flags rapid yaw changes may trigger. This is the same behavior as any
mouse sensitive flight mod. The mod is client side only with no server component. Dedicated server players
do not receive it, as native mods are not built for dedicated servers yet.

## For developers

### How the roll works

Camera roll is applied inside the level render pose:

```text
clip = projection · (bob · spin) · view_matrix(yaw, pitch) · model_matrix
```

The `Game::spinning_matrix` method returns a Z rotation of the current bank. This means terrain, sky,
entities and frustum culling all rotate around the screen center, while `game.yaw` and `game.pitch`
remain unchanged.

The bank comes from mouse X input. Moving the mouse left or right sets a target roll, and the frame hook
smoothly interpolates toward it. Keys A and D provide rudder yaw. When the mouse stops moving, the roll
naturally levels back to zero.

Since the roll lives in the projection matrix, every model on screen tilts with the world, including the
player avatar. An earlier version pinned the avatar to the camera using a `base_transform` mixin, but
this was removed during the 2026-11-20 Nightly migration when that target no longer existed.

| `[[native.mixin]]` target | Kind | Helper |
|---|---|---|
| `render_frame_pre::Game::native_render_frame_pre` | `HEAD` | `frame_helper.rs` handles roll smoothing and physics |
| `game::fov::Game::spinning_matrix` | `RETURN` | `spin_helper.rs` applies the camera bank and captures the render view |
| `game::pointer_capture::Game::apply_mouse_look` | `OVERWRITE` | `turn_helper.rs` handles mouse roll, pitch and A/D rudder |

The flight physics and atomic state live in `aerobatica.rs`. This file is injected as the `aerobatica`
module inside `crates/horizon-client/src/client_lib.rs` (the `horizon-client-lib` crate used since
Nightly 2026-11-10). It also serves as this crate's library target so the math can be tested standalone.

### Files

```
horizon-mod.toml        manifest with [native] pins, [[native.mixin]] hooks, and [[native.module]]
Cargo.toml              crate metadata
src/lib.rs              crate root with mod_api Hooks entry point
src/aerobatica.rs       flight physics, camera roll, and rudder logic, injected as a support module
src/mixins/             one helper per hook (frame, spin, turn)
rust-toolchain.toml     Rust version for cargo test
tools/                  build and test helpers
```

### Build and test

```sh
cargo test                                   # run flight math tests standalone

# For the launcher build, move extra files out of the way first:
#   mv rust-toolchain.toml tools target /tmp/
horizon-launcher native-build . --instance "My Instance" --trust-native-code
#   the launcher compiles the mod against the prebuilt game
#   then restore: mv /tmp/* .
```

On Windows, set `core.autocrlf=false` so source file hashes match the Linux commit pins.

## Known Issues

**Possible pitch instability at extreme angles.** Very rarely, when the camera is pitched to an extreme
angle (nearly straight up or down), the pitch-up or pitch-down response may briefly feel unresponsive
or stuck. This may be caused by the pitch value reaching the boundary of its internal representation,
where the flight model's loop-detection logic interacts with the angle wrap-around. The normalization
fix prevents the severe drift that caused major bugs, but this edge case at the boundary remains a
possible issue that has not been fully eliminated. If it occurs, moving the mouse to bring the pitch
back to a moderate angle should resolve it quickly.

**Possible unintended extreme flight speed increase.** A fix has been 
implemented, but since the bug was so rare, there has been insufficient 
testing to completely rule out the bug post-fix. However, as of the writing
of this, this bug has not been observed since the last attempted fix. If you
experience this bug while on a strict server, it may trigger anticheat, so
beware.

## License

MIT license, matching Horizon's own license for native mods. This project does not contain any of
Mojang's code, assets or data.