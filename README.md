# Bevy Fog of War

[中文文档](README_CN.md) | English

![Fog of war screenshot](https://github.com/user-attachments/assets/d8821536-7c91-4527-9425-c64ee5252b20)

[![CI](https://github.com/foxzool/bevy_fog_of_war/workflows/CI/badge.svg)](https://github.com/foxzool/bevy_fog_of_war/actions)
[![Crates.io](https://img.shields.io/crates/v/bevy_fog_of_war)](https://crates.io/crates/bevy_fog_of_war)
[![Downloads](https://img.shields.io/crates/d/bevy_fog_of_war)](https://crates.io/crates/bevy_fog_of_war)
[![Documentation](https://docs.rs/bevy_fog_of_war/badge.svg)](https://docs.rs/bevy_fog_of_war)
[![MIT/Apache 2.0](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](LICENSE)

`bevy_fog_of_war` is a 2D fog-of-war plugin for Bevy with GPU rendering, chunk-based streaming, explored-area snapshots, and persistence helpers.

## Highlights

- Chunk-based fog processing suitable for large 2D maps.
- Circle, square, and cone `VisionSource` shapes.
- Three visibility states: `Unexplored`, `Explored`, and `Visible`.
- `Capturable` entities that can be revealed and snapshotted.
- Atomic fog reset with `FogResetSuccess` / `FogResetFailed` messages.
- Save/load flow through Bevy messages.
- Optional JSON / MessagePack / bincode serialization.
- Optional gzip / LZ4 / Zstd file helpers in `persistence_utils`.

## Compatibility

| bevy_fog_of_war | Bevy |
| --- | --- |
| 0.4.x | 0.19.0-rc.2 |

## Installation

```toml
[dependencies]
bevy = "0.19.0-rc.2"
bevy_fog_of_war = "0.4"
```

Default features enable `format-bincode`.

### Optional feature flags

```toml
# Add MessagePack support
bevy_fog_of_war = { version = "0.4", features = ["format-messagepack"] }

# Add JSON support
bevy_fog_of_war = { version = "0.4", features = ["format-json"] }

# Add compression helpers for persistence_utils
bevy_fog_of_war = { version = "0.4", features = ["compression-zstd"] }

# Everything: all formats + all compression helpers
bevy_fog_of_war = { version = "0.4", features = ["all-formats"] }
```

## Quick start

Most users can import everything they need with:

```rust
use bevy_fog_of_war::prelude::*;
```

Minimal setup:

```rust
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy_fog_of_war::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(FogOfWarPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, mut fog: ResMut<FogMapSettings>) {
    commands.spawn((Camera2d, FogOfWarCamera, VisionSource::circle(120.0)));

    commands.spawn((
        Sprite {
            color: Color::srgb(0.2, 0.8, 0.8),
            custom_size: Some(Vec2::splat(48.0)),
            ..default()
        },
        Transform::from_xyz(160.0, 0.0, 0.0),
        Capturable,
    ));

    *fog = FogMapSettings {
        enabled: true,
        chunk_size: UVec2::splat(256),
        texture_resolution_per_chunk: UVec2::splat(512),
        fog_color_unexplored: Color::BLACK,
        fog_color_explored: bevy::color::palettes::basic::GRAY.into(),
        vision_clear_color: Color::NONE,
        fog_texture_format: TextureFormat::R8Unorm,
        snapshot_texture_format: TextureFormat::Rgba8UnormSrgb,
    };
}
```

## Core concepts

### `FogOfWarCamera`

Mark the 2D camera that should drive fog rendering:

```rust
commands.spawn((Camera2d, FogOfWarCamera));
```

You can also attach a `VisionSource` to the camera if you want the camera/player to reveal nearby areas.

### `VisionSource`

Use built-in constructors for common shapes:

```rust
commands.spawn((Transform::default(), VisionSource::circle(120.0)));
commands.spawn((Transform::default(), VisionSource::square(120.0)));
commands.spawn((
    Transform::default(),
    VisionSource::cone(180.0, 0.0, std::f32::consts::FRAC_PI_2),
));
```

### `Capturable`

Add `Capturable` to entities that should only be visible after they have been discovered by fog-of-war vision.

## Resetting fog

Send `ResetFogOfWar` to clear explored state and rebuild the fog textures without respawning your world:

```rust
use bevy::prelude::*;
use bevy_fog_of_war::prelude::*;

fn reset_on_keypress(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut reset: MessageWriter<ResetFogOfWar>,
) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        reset.write(ResetFogOfWar);
    }
}

fn handle_reset_messages(
    mut success: MessageReader<FogResetSuccess>,
    mut failed: MessageReader<FogResetFailed>,
) {
    for event in success.read() {
        info!(
            "fog reset finished in {}ms ({} chunks)",
            event.duration_ms, event.chunks_reset
        );
    }

    for event in failed.read() {
        error!("fog reset failed after {}ms: {}", event.duration_ms, event.error);
    }
}
```

## Persistence via Bevy messages

The runtime API uses Bevy messages plus raw bytes. Compression-oriented file helpers live in `persistence_utils`.

### Save

```rust
use bevy::prelude::*;
use bevy_fog_of_war::prelude::*;

fn request_save(mut save: MessageWriter<SaveFogOfWarRequest>) {
    save.write(SaveFogOfWarRequest {
        include_texture_data: true,
        format: None, // auto-picks the best enabled SerializationFormat
    });
}

fn handle_saved(mut events: MessageReader<FogOfWarSaved>) {
    for event in events.read() {
        let ext = match event.format {
            #[cfg(feature = "format-json")]
            SerializationFormat::Json => "json",
            #[cfg(feature = "format-messagepack")]
            SerializationFormat::MessagePack => "msgpack",
            #[cfg(feature = "format-bincode")]
            SerializationFormat::Bincode => "bincode",
        };

        let path = format!("fog_save.{ext}");
        std::fs::write(&path, &event.data).expect("failed to write fog save");
        info!("saved {} chunks to {}", event.chunk_count, path);
    }
}
```

### Load

```rust
use bevy::prelude::*;
use bevy_fog_of_war::prelude::*;

fn request_load(mut load: MessageWriter<LoadFogOfWarRequest>) {
    if let Ok(data) = std::fs::read("fog_save.bincode") {
        load.write(LoadFogOfWarRequest {
            data,
            format: None, // auto-detect from bytes
        });
    }
}

fn handle_loaded(mut events: MessageReader<FogOfWarLoaded>) {
    for event in events.read() {
        info!("loaded {} chunks", event.chunk_count);

        for warning in &event.warnings {
            warn!("load warning: {warning}");
        }
    }
}
```

## Persistence file helpers

If you want extension-based save/load helpers or compressed files, use `bevy_fog_of_war::persistence_utils`:

```rust
use bevy_fog_of_war::persistence::FogOfWarSaveData;
use bevy_fog_of_war::persistence_utils::{load_fog_data, save_fog_data, FileFormat};

# fn example(save_data: &FogOfWarSaveData) -> Result<(), Box<dyn std::error::Error>> {
save_fog_data(save_data, "fog_save.bincode", FileFormat::Bincode)?;
let restored = load_fog_data("fog_save.bincode", None)?;
# let _ = restored;
# Ok(())
# }
```

Compressed variants such as `FileFormat::BincodeZstd` or `FileFormat::MessagePackLz4` are available when the matching compression features are enabled.

## Examples

```bash
cargo run --example simple_2d
cargo run --example playground
```

- `simple_2d`: minimal gameplay scene plus save/load demonstration.
- `playground`: interactive demo with camera controls, live settings text, reset flow, and persistence.

## API docs

- Crate docs: <https://docs.rs/bevy_fog_of_war>
- Source examples: [`examples/`](examples/)

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Contributing

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you shall be dual licensed as above, without any additional terms or conditions.
