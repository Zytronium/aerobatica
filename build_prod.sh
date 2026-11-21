#!/bin/fish
# Build the .hmod distributable package for the Gyroflight native mod.
# The .hmod is a zip of the mod source: horizon-mod.toml, Cargo.toml, and src/.
# The launcher compiles the native mod on the player's computer.

python3 -c "
import zipfile, os, glob

hmod_path = 'target/horizon-mod/gyroflight-1.0.1.hmod'
os.makedirs(os.path.dirname(hmod_path), exist_ok=True)

files = [
    'horizon-mod.toml',
    'Cargo.toml',
    'src/lib.rs',
    'src/gyroflight.rs',
]
for f in sorted(glob.glob('src/mixins/*.rs')):
    files.append(f)

with zipfile.ZipFile(hmod_path, 'w', zipfile.ZIP_DEFLATED) as zf:
    for f in files:
        zf.write(f)

print(f'Created {hmod_path} ({os.path.getsize(hmod_path)} bytes, {len(files)} files)')
"

# Optionally build into the game for testing:
# ~/Applications/horizon-launcher native-build . --instance "Horizon" --trust-native-code
