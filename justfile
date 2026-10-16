# Solar Magic task runner. `just` lists the recipes.

# roms/ lives in the main checkout; worktrees share it through the git common dir.
roms := env("SOLAR_ROMS", parent_directory(`git rev-parse --path-format=absolute --git-common-dir 2>/dev/null || echo "$PWD/.git"`) / "roms")
smw := roms / "Super Mario World (USA).sfc"
dist := justfile_directory() / "target" / "dist"

# List the recipes
default:
    @just --list

# Build the workspace
build:
    cargo build --workspace

# Unit tests (real-ROM tests skip)
test:
    cargo test --workspace

# Unit tests plus the real-ROM tests
test-rom:
    SMW_ROM="{{ smw }}" LM_ROM="{{ roms }}/Patched_Quickie World 2/QW2.sfc" LM3_ROM="{{ roms }}/lm371/a-unchanged.sfc" LM_MWL="{{ roms }}/lm371/a-unchanged-105.mwl" cargo test --workspace

clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all --check

# 1000-LOC file limit, lint hygiene
xtask-check:
    cargo xtask check

doc:
    cargo doc --workspace --no-deps --all-features

# Everything CI runs on each OS
check: fmt-check clippy xtask-check
    cargo test --workspace --all-features
    cargo doc --workspace --no-deps --all-features

# Open the editor on a level
run level="105" rom=smw:
    cargo run -p solar-magic -- "{{ rom }}" {{ level }}

# Open the editor and play the level
play level="105" rom=smw:
    cargo run -p solar-magic --features gamepad -- "{{ rom }}" {{ level }} --play

# Run a solar-cli command, e.g. `just solar info "$ROM"`
solar *args:
    cargo solar {{ args }}

# Open-source SNES test ROMs vs their screenshots
test-roms:
    cargo xtask fetch-test-roms
    SNES_TEST_ROMS=target/test-roms/SNES cargo test -p snes-core --release --test test_roms

# Build "Solar Magic.app" in target/dist (arch: native | universal)
[macos]
app arch="native":
    #!/usr/bin/env bash
    set -euo pipefail
    version="$(cargo pkgid -p solar-magic | sed 's/.*[#@]//')"
    case "{{ arch }}" in
      native)
        cargo build --release -p solar-magic --features gamepad
        bin=target/release/solar-magic ;;
      universal)
        rustup target add aarch64-apple-darwin x86_64-apple-darwin
        cargo build --release -p solar-magic --features gamepad --target aarch64-apple-darwin
        cargo build --release -p solar-magic --features gamepad --target x86_64-apple-darwin
        bin=target/universal/solar-magic
        mkdir -p target/universal
        lipo -create -output "$bin" \
          target/aarch64-apple-darwin/release/solar-magic \
          target/x86_64-apple-darwin/release/solar-magic ;;
      *) echo "arch must be native or universal" >&2; exit 1 ;;
    esac
    app="{{ dist }}/Solar Magic.app"
    rm -rf "$app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp "$bin" "$app/Contents/MacOS/solar-magic"
    sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
    # Ad-hoc by default; set SIGN_IDENTITY="Developer ID Application: ..." to distribute.
    codesign --force --options runtime --sign "${SIGN_IDENTITY:--}" "$app"
    echo "$app"

# Package the app as a drag-to-Applications DMG (arch: native | universal)
[macos]
dmg arch="native": (app arch)
    #!/usr/bin/env bash
    set -euo pipefail
    version="$(cargo pkgid -p solar-magic | sed 's/.*[#@]//')"
    suffix="$([ "{{ arch }}" = universal ] && echo universal || uname -m)"
    out="{{ dist }}/SolarMagic-$version-$suffix.dmg"
    stage="$(mktemp -d)"
    trap 'rm -rf "$stage"' EXIT
    cp -R "{{ dist }}/Solar Magic.app" "$stage/"
    ln -s /Applications "$stage/Applications"
    hdiutil create -volname "Solar Magic" -srcfolder "$stage" -ov -format UDZO "$out"
    echo "$out"
