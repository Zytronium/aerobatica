# Gyroflight

**Fly an elytra like an aircraft.** Put on elytra, jump, and the mouse becomes your flight stick: moving it
left and right **rolls the aircraft** (banking), moving it up and down pitches — and because that pull goes
through your bank, a wing-down tilt carves a turn. **A and D keys act as rudder** for yaw, so you can
coordinate turns independently. Push the nose past straight up and you loop. The camera smoothly follows
the aircraft attitude, and the view levels out when you land.

Written for the feel of the Minecraft Java mod *Do a Barrel Roll*, on Horizon's native hook points.

## Controls

| While gliding | What it does |
|---|---|
| Mouse left / right | **Roll**: banks the aircraft left or right |
| Mouse up / down | Pitch: climb and dive; through the bank, a pull-up carves |
| A key | **Yaw left** (rudder) |
| D key | **Yaw right** (rudder) |
| Anything else | Vanilla — walking, flying and swimming controls are untouched |

**The bank is the barrel roll.** Keep rolling in one direction and the world keeps leaning over,
further and further, until you have rolled right around and come back to level — a full barrel roll,
done with nothing but the mouse.

Nothing to bind. **Options > Mods > Gyroflight > Config** has two settings: **Invert pitch**, and **Camera
smoothness** (0 is instant, 1 is very smooth; roll, pitch and yaw all ease toward their target, pitch a
little more than the others).

## Play it

**Through the Horizon launcher** (the usual way for a native mod):

```sh
horizon-launcher native-build . --instance "My Instance" --trust-native-code
```

The launcher downloads its own Rust and compiles the mod against the prebuilt game; the first build takes
20 to 30 minutes, later ones are incremental. It asks for hook points v3, so it builds on any Horizon
that has them rather than one exact commit.

**Important:** The launcher only accepts specific files in the mod directory. Before building, move any
extra files (like `rust-toolchain.toml`, `tools/`, `target/`, etc.) out of the way, or run the command
from a clean copy of the repo. The launcher rejects files like `rust-toolchain.toml`, `mixins/`, and
other non-standard paths.

Native mods are unsandboxed code that runs with your rights, and this one is not reviewed by the Horizon
registry — the launcher will say so in red before it builds. The source is a handful of `.rs` files under
`src/`; read them first.

Either way: `/give @s minecraft:elytra 1`, jump off something high, and hold Space to glide.

## Getting it running

Gyroflight only moves the camera and the player's own model; the server never hears about the roll, so it is safe
on a strict server — but the rudder and the carved turns do change where you go, so an anticheat that
dislikes fast yaw changes may complain, the same as with any mouse-sensitive mod. It is client-side only: no
server component, and dedicated-server players just do not get it (native mods are not built for dedicated
servers yet).

---

## For developers

### How the roll is drawn

Camera roll rides in the level's render pose:

```text
clip = projection · (bob · spin) · view_matrix(yaw, pitch) · model_matrix
```

`Game::spinning_matrix` gets multiplied by a Z rotation of the current bank, so terrain, sky, entities and
frustum culling all turn around the screen centre, while `game.yaw` and `game.pitch` stay clean.

The bank is driven by mouse X input: moving the mouse left/right sets a target roll, and the frame hook
smoothly interpolates toward it. A/D keys provide rudder yaw. When you stop moving the mouse, the roll
naturally levels out — the barrel roll, with nothing but the mouse.

Because that roll is in the projection, every model on screen tilts with the world, the player's own avatar
included. In earlier versions the avatar was pinned to the camera via a `base_transform` mixin, but this
was removed in the 2026-11-20 Nightly migration as the target no longer exists.

| `[[native.mixin]]` target | Kind | Helper |
|---|---|---|
| `render_frame_pre::Game::native_render_frame_pre` | `HEAD` | `frame_helper.rs` — roll smoothing and physics |
| `game::fov::Game::spinning_matrix` | `RETURN` | `spin_helper.rs` — the camera bank, render view capture |
| `game::pointer_capture::Game::apply_mouse_look` | `OVERWRITE` | `turn_helper.rs` — mouse roll/pitch + A/D rudder |

Roll physics and the atomics behind it are in `gyroflight.rs`, injected as the `gyroflight` module of
`crates/horizon-client/src/client_lib.rs` (the `horizon-client-lib` crate the game is compiled from since
Nightly 2026-11-10), and it is also this crate's library so the numbers can be tested on their own.

### Files

```
horizon-mod.toml        the manifest: [native] pins, [[native.mixin]] hooks, [[native.module]]
Cargo.toml              crate metadata
src/lib.rs              crate root (mod_api Hooks entry point)
src/gyroflight.rs       flight physics, camera roll, rudder — injected as a support module
src/mixins/             one helper per hook (frame, spin, turn)
rust-toolchain.toml     Rust version for cargo test
tools/                  build and test helpers
```

### Build, test, prove

```sh
cargo test                                   # the flight maths, standalone

# For the launcher build, move extra files out of the way first:
#   mv rust-toolchain.toml tools target compatibility.json /tmp/
horizon-launcher native-build . --instance "My Instance" --trust-native-code
#   the launcher compiles the mod against the prebuilt game
#   then restore: mv /tmp/* .
```

Windows notes: `core.autocrlf=false` so source hashes match the Linux pin.

## License

MIT, Horizon's own licence for a Horizon mod. Contains none of Mojang's code, assets or data.