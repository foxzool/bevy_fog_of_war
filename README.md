# Bevy Fog of War

A fog of war implementation for the Bevy game engine. This crate provides a simple way to add fog of war effects to
your 2D games, with support for multiple light sources, smooth transitions, and explored area tracking.

## Features

- 2D fog of war with smooth transitions
- Multiple dynamic light sources
- Adjustable fog density and color
- Explored area tracking with configurable visibility
- Camera movement controls
- Efficient GPU-based implementation using WGSL shaders

## Usage

To use `bevy_fog_of_war` in your project, follow these steps:

1.  **Add the plugin to your app:**

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

2.  **Add `FogOfWarCamera` to your camera:**

    The plugin needs to know which camera is used for the fog of war effect. Add the `FogOfWarCamera` component to your main 2D camera entity.

    ```rust
    use bevy::prelude::*;
    use bevy_fog_of_war::prelude::FogOfWarCamera;

    fn setup(mut commands: Commands) {
        commands.spawn((Camera2d, FogOfWarCamera));
    }
    ```

3.  **Add `VisionSource` to entities:**

    Entities that should reveal the map need a `VisionSource` component. You can create different shapes for the vision area, such as circles, squares, or cones.

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

   4.  **(Optional) Add `Capturable` to entities:**

       If you have entities that should only become visible when a `VisionSource` overlaps them (and remain visible once discovered, depending on your fog settings), add the `Capturable` component to them.

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

Check the [examples](examples/) directory for more detailed usage scenarios, including dynamic vision sources and different vision shapes.

## Compatibility

| Bevy Version | Plugin Version |
|--------------|----------------|
| 0.16         | 0.2.0          |
| 0.15         | 0.1.0          |

## License

This project is licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

## Contributing

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as
defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.