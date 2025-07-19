# Bevy Fog of War

[中文文档](README_CN.md) | English

![截屏2025-05-23 19 23 05](https://github.com/user-attachments/assets/d8821536-7c91-4527-9425-c64ee5252b20)

[![CI](https://github.com/foxzool/bevy_fog_of_war/workflows/CI/badge.svg)](https://github.com/foxzool/bevy_fog_of_war/actions)
[![Crates.io](https://img.shields.io/crates/v/bevy_fog_of_war)](https://crates.io/crates/bevy_fog_of_war)
[![Downloads](https://img.shields.io/crates/d/bevy_fog_of_war)](https://crates.io/crates/bevy_fog_of_war)
[![Documentation](https://docs.rs/bevy_fog_of_war/badge.svg)](https://docs.rs/bevy_fog_of_war)
[![MIT/Apache 2.0](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/Seldom-SE/seldom_pixel#license)

A fog of war implementation for the Bevy game engine. This crate provides a simple way to add fog of war effects to
your 2D games, with support for multiple light sources, smooth transitions, and explored area tracking.

## Features

- 2D fog of war with smooth transitions and customizable colors.
- Support for multiple dynamic vision sources with various shapes.
- Explored area tracking, with options for how explored areas remain visible.
- Chunk-based map processing for efficient updates, suitable for large maps.
- Snapshot system for persisting explored fog data.
- Atomic fog reset functionality with success/failure event notifications.
- Save/load functionality for persisting fog data per character or save file.
- Multiple serialization formats: JSON, MessagePack, bincode with compression support.
- Server-friendly serialization for fog state with automatic format detection.
- Highly configurable via the `FogMapSettings` resource.
- Efficient GPU-based implementation using WGSL compute shaders.

## Usage

To use `bevy_fog_of_war` in your project, follow these steps:

(You can import most commonly used items via `use bevy_fog_of_war::prelude::*;`)

1. **Add the plugin to your app:**

   Add the `FogOfWarPlugin` to your Bevy `App`:

   ```rust
   use bevy::prelude::*;
   use bevy_fog_of_war::FogOfWarPlugin;

   fn main() {
       App::new()
           .add_plugins(DefaultPlugins)
           .add_plugins(FogOfWarPlugin) // Add the fog of war plugin
           // ... other setup ...
           .run();
   }
   ```

2. **Add `FogOfWarCamera` to your camera:**

   The plugin needs to know which camera is used for the fog of war effect. Add the `FogOfWarCamera` component to your
   main 2D camera entity.

   ```rust
   use bevy::prelude::*;
   use bevy_fog_of_war::prelude::FogOfWarCamera;

   fn setup(mut commands: Commands) {
       commands.spawn((Camera2d, FogOfWarCamera));
   }
   ```

3. **Add `VisionSource` to entities:**

   Entities that should reveal the map need a `VisionSource` component. You can create different shapes for the vision
   area, such as circles, squares, or cones.

   ```rust
   use bevy::prelude::*;
   use bevy_fog_of_war::prelude::VisionSource;

   fn setup_entities(mut commands: Commands) {
       // Spawn an entity with a circular vision source
       commands.spawn((Transform::from_xyz(0.0, 0.0, 0.0), VisionSource::circle(200.0)));

       // Spawn another entity with a square vision source
       commands.spawn((Transform::from_xyz(100.0, 50.0, 0.0), VisionSource::square(150.0)));
   }
   ```

4. **(Optional) Add `Capturable` to entities:**

   If you have entities that should only become visible when a `VisionSource` overlaps them (and remain visible once
   discovered, depending on your fog settings), add the `Capturable` component to them.

   ```rust
   use bevy::prelude::*;
   use bevy_fog_of_war::prelude::Capturable;

   fn setup_objects(mut commands: Commands) {
       commands.spawn((
           Sprite {
                color: Color::srgb(0.2, 0.8, 0.8),
                custom_size: Some(Vec2::new(60.0, 60.0)),
                ..default()
           },
           Capturable, // This entity will be revealed by vision sources
       ));
   }
   ```

5. **Customize `FogMapSettings` (Optional):**

   You can customize the fog of war behavior by inserting a `FogMapSettings` resource. Here's an example of how to
   configure it:

   ```rust
   use bevy::prelude::*;
   use bevy_fog_of_war::prelude::*;
   use bevy::render::render_resource::TextureFormat;

   fn setup_fog_settings(mut commands: Commands) {
       commands.insert_resource(FogMapSettings {
           enabled: true,  // Enable/disable fog of war effect
           chunk_size: UVec2::new(256, 256),  // Size of each chunk in world units
           texture_resolution_per_chunk: UVec2::new(512, 512),  // Texture resolution per chunk
           fog_color_unexplored: Color::rgba(0.1, 0.1, 0.1, 0.9),  // Color for unexplored areas
           fog_color_explored: Color::rgba(0.3, 0.3, 0.3, 0.5),   // Color for explored but not visible areas
           vision_clear_color: Color::NONE,  // Clear color for visible areas (usually transparent)
           fog_texture_format: TextureFormat::R8Unorm,  // Texture format for fog
           snapshot_texture_format: TextureFormat::R8Unorm  // Texture format for snapshots
       });
   }
   ```

   Then add the system to your app:

   ```rust
   .add_systems(Startup, setup_fog_settings)
   ```

Check the [examples](examples/) directory for more detailed usage scenarios, including dynamic vision sources and
different vision shapes.

## Resetting Fog of War

You can programmatically reset all fog of war data (explored areas, visibility states, and texture data) without
despawning entities or cameras. This is useful when changing scenes or restarting levels:

```rust
use bevy::prelude::*;
use bevy_fog_of_war::prelude::*;

fn reset_on_keypress(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut reset_events: EventWriter<ResetFogOfWarEvent>,
) {
    if keyboard_input.just_pressed(KeyCode::KeyR) {
        // Reset all fog of war data
        reset_events.write(ResetFogOfWarEvent);
    }
}

// Add this system to your app
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(FogOfWarPlugin)
        .add_systems(Update, reset_on_keypress)
        .run();
}
```

### Reset Event Notifications

The plugin provides events to notify you when reset operations complete:

```rust
use bevy::prelude::*;
use bevy_fog_of_war::prelude::*;

fn handle_fog_reset_events(
    mut success_events: EventReader<FogResetSuccessEvent>,
    mut failure_events: EventReader<FogResetFailedEvent>,
) {
    for event in success_events.read() {
        info!("✅ Fog reset completed successfully! Duration: {}ms, Chunks reset: {}", 
              event.duration_ms, event.chunks_reset);
    }

    for event in failure_events.read() {
        error!("❌ Fog reset failed! Duration: {}ms, Error: {}", 
               event.duration_ms, event.error);
    }
}

// Add the event handler to your app
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(FogOfWarPlugin)
        .add_systems(Update, (reset_on_keypress, handle_fog_reset_events))
        .run();
}
```

The reset functionality:

- Clears all explored areas
- Resets all chunk visibility states to unexplored
- Resets texture data to initial state
- Preserves all entities, cameras, and vision sources
- Allows for seamless scene transitions
- Provides success/failure events with timing information
- Includes automatic rollback on failure

Check the [`playground.rs`](examples/playground.rs) and [`simple_2d.rs`](examples/simple_2d.rs) examples for complete
demonstrations.

## Serialization Formats

The plugin supports multiple serialization formats for optimal performance and compatibility. Choose the format that best suits your needs:

### Available Formats

| Format | Size | Speed | Compatibility | Use Case |
|--------|------|-------|---------------|----------|
| **JSON** | Largest | Slow | Universal | Debug, web APIs, human-readable |
| **MessagePack** | Small | Fast | Cross-language | Network, storage-efficient |
| **bincode** | Smallest* | Fastest | Rust-only | Local saves, performance-critical |

*When combined with compression

### Feature Flags

Add the desired formats to your `Cargo.toml`:

```toml
[dependencies]
bevy_fog_of_war = { version = "0.2.1", features = ["format-messagepack", "format-bincode"] }

# Or enable everything:
bevy_fog_of_war = { version = "0.2.1", features = ["all-formats"] }

# Compression support:
bevy_fog_of_war = { version = "0.2.1", features = ["all-formats", "all-compression"] }
```

### Performance Comparison

Based on benchmarks with typical fog data:

```
Format              | File Size | Save Time | Load Time
--------------------|-----------|-----------|----------
JSON                | 100%      | 100%      | 100%
JSON + gzip         | 65%       | 120%      | 110%
MessagePack         | 45%       | 30%       | 40%
MessagePack + LZ4   | 35%       | 35%       | 45%
bincode             | 30%       | 15%       | 20%
bincode + Zstandard | 25%       | 20%       | 25%
```

### Usage Examples

#### Basic Format Selection

```rust
use bevy_fog_of_war::prelude::*;

// Save in different formats
save_data_to_file(&fog_data, "save.json", FileFormat::Json)?;
save_data_to_file(&fog_data, "save.msgpack", FileFormat::MessagePack)?;
save_data_to_file(&fog_data, "save.bincode", FileFormat::Bincode)?;

// Load with automatic format detection
let data = load_data_from_file::<FogOfWarSaveData>("save.msgpack", None)?;
```

#### Compressed Formats

```rust
// High compression for storage
save_data_to_file(&fog_data, "save.msgpack.zst", FileFormat::MessagePackZstd)?;

// Fast compression for frequent saves
save_data_to_file(&fog_data, "save.bincode.lz4", FileFormat::BincodeLz4)?;

// Automatic format detection works with compression
let data = load_data_from_file::<FogOfWarSaveData>("save.msgpack.zst", None)?;
```

#### Smart Loading Priority

The plugin automatically tries formats in optimal order:

```rust
// Tries: .bincode.zst → .msgpack.zst → .bincode → .msgpack → .json
let result = load_fog_data("character_save", None)?;
```

### Format Recommendations

- **Development**: Use JSON for debugging and manual inspection
- **Production Local**: Use bincode for fastest local saves
- **Network/API**: Use MessagePack for efficient transmission
- **Long-term Storage**: Use compressed formats (Zstandard for best compression)
- **Cross-platform**: Use MessagePack for compatibility with other languages

## Persistence

The plugin supports saving and loading fog of war data, allowing you to persist explored areas across game sessions or per character/save file.

### Saving Fog of War Data

```rust
use bevy::prelude::*;
use bevy_fog_of_war::prelude::*;

fn save_fog_data(
    mut save_events: EventWriter<SaveFogOfWarRequest>,
) {
    // Request to save fog data for a specific character
    save_events.write(SaveFogOfWarRequest {
        character_id: "player_1".to_string(),
        include_texture_data: true, // Include texture data for partial visibility
    });
}

fn handle_save_complete(
    mut events: EventReader<FogOfWarSaved>,
) {
    for event in events.read() {
        println!("Saved {} chunks for character {}", 
                 event.chunk_count, event.character_id);
        
        // Parse the JSON data to a struct for flexible saving
        let save_data: FogOfWarSaveData = serde_json::from_str(&event.data).unwrap();
        
        // Save in multiple formats for demonstration
        let formats = vec![
            (FileFormat::Json, "json"),
            (FileFormat::MessagePack, "msgpack"),
            (FileFormat::BincodeZstd, "bincode.zst"), // Compressed for storage
        ];
        
        for (format, ext) in formats {
            let filename = format!("fog_save_{}.{}", event.character_id, ext);
            if let Err(e) = save_data_to_file(&save_data, &filename, format) {
                eprintln!("Failed to save {}: {}", filename, e);
            } else {
                println!("Saved to {} ({:?})", filename, format);
            }
        }
    }
}
```

### Loading Fog of War Data

```rust
use bevy::prelude::*;
use bevy_fog_of_war::prelude::*;

fn load_fog_data(
    mut load_events: EventWriter<LoadFogOfWarRequest>,
) {
    // Option 1: Load with automatic format detection
    match load_data_from_file::<FogOfWarSaveData>("player_save.msgpack", None) {
        Ok(save_data) => {
            let json_data = serde_json::to_string(&save_data).unwrap();
            load_events.write(LoadFogOfWarRequest {
                character_id: "player_1".to_string(),
                data: json_data,
            });
        }
        Err(e) => eprintln!("Failed to load: {}", e),
    }
    
    // Option 2: Try multiple formats with priority (fastest first)
    let format_priorities = [
        ("player_save.bincode.zst", Some(FileFormat::BincodeZstd)),
        ("player_save.msgpack", Some(FileFormat::MessagePack)),
        ("player_save.json", Some(FileFormat::Json)),
    ];
    
    for (filename, format) in format_priorities {
        if std::path::Path::new(filename).exists() {
            match load_data_from_file::<FogOfWarSaveData>(filename, format) {
                Ok(save_data) => {
                    println!("Loading from '{}' (auto-detected format)", filename);
                    let json_data = serde_json::to_string(&save_data).unwrap();
                    load_events.write(LoadFogOfWarRequest {
                        character_id: "player_1".to_string(),
                        data: json_data,
                    });
                    break;
                }
                Err(e) => println!("Failed to load {}: {}", filename, e),
            }
        }
    }
}

fn handle_load_complete(
    mut events: EventReader<FogOfWarLoaded>,
) {
    for event in events.read() {
        println!("Loaded {} chunks for character {}", 
                 event.chunk_count, event.character_id);
        
        if !event.warnings.is_empty() {
            println!("Warnings: {:?}", event.warnings);
        }
    }
}
```

### Server Integration

The persistence system is designed to work with server-side storage:

```rust
// Example server integration
async fn save_to_server(character_id: &str, fog_data: &str) {
    // Send fog data to your game server
    let response = reqwest::Client::new()
        .post("https://api.yourgame.com/fog-of-war/save")
        .json(&serde_json::json!({
            "character_id": character_id,
            "fog_data": fog_data,
        }))
        .send()
        .await
        .unwrap();
}

async fn load_from_server(character_id: &str) -> String {
    // Fetch fog data from your game server
    let response = reqwest::Client::new()
        .get(format!("https://api.yourgame.com/fog-of-war/{}", character_id))
        .send()
        .await
        .unwrap();
    
    response.text().await.unwrap()
}
```

See the [`persistence.rs`](examples/persistence.rs) example for a complete demonstration of saving and loading fog data.

## Compatibility

| Bevy Version | Plugin Version |
|--------------|----------------|
| 0.16         | 0.2.1          |
| 0.15         | 0.1.0          |

## License

This project is licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

## Contributing

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as
defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
