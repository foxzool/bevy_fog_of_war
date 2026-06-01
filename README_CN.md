# Bevy 战争迷雾

中文 | [English](README.md)

![战争迷雾截图](https://github.com/user-attachments/assets/d8821536-7c91-4527-9425-c64ee5252b20)

[![CI](https://github.com/foxzool/bevy_fog_of_war/workflows/CI/badge.svg)](https://github.com/foxzool/bevy_fog_of_war/actions)
[![Crates.io](https://img.shields.io/crates/v/bevy_fog_of_war)](https://crates.io/crates/bevy_fog_of_war)
[![Downloads](https://img.shields.io/crates/d/bevy_fog_of_war)](https://crates.io/crates/bevy_fog_of_war)
[![Documentation](https://docs.rs/bevy_fog_of_war/badge.svg)](https://docs.rs/bevy_fog_of_war)
[![MIT/Apache 2.0](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](LICENSE)

`bevy_fog_of_war` 是一个面向 Bevy 的 2D 战争迷雾插件，提供 GPU 渲染、分块流式处理、已探索区域快照，以及持久化辅助工具。

## 亮点

- 基于区块的迷雾处理，适合大地图。
- 支持圆形、方形、扇形 `VisionSource`。
- 三种可见性状态：`Unexplored`、`Explored`、`Visible`。
- 支持通过 `Capturable` 控制实体被发现与快照。
- 原子化迷雾重置，并通过 `FogResetSuccess` / `FogResetFailed` 返回结果。
- 通过 Bevy message 完成保存 / 加载。
- 可选 JSON / MessagePack / bincode 序列化。
- `persistence_utils` 提供可选 gzip / LZ4 / Zstd 文件辅助能力。

## 兼容性

| bevy_fog_of_war | Bevy |
| --- | --- |
| 0.4.x | 0.19.0-rc.2 |

## 安装

```toml
[dependencies]
bevy = "0.19.0-rc.2"
bevy_fog_of_war = "0.4"
```

默认启用 `format-bincode`。

### 可选 feature

```toml
# 增加 MessagePack 支持
bevy_fog_of_war = { version = "0.4", features = ["format-messagepack"] }

# 增加 JSON 支持
bevy_fog_of_war = { version = "0.4", features = ["format-json"] }

# 为 persistence_utils 增加压缩辅助能力
bevy_fog_of_war = { version = "0.4", features = ["compression-zstd"] }

# 全部开启：所有格式 + 所有压缩辅助
bevy_fog_of_war = { version = "0.4", features = ["all-formats"] }
```

## 快速开始

大多数场景直接：

```rust
use bevy_fog_of_war::prelude::*;
```

最小接入示例：

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

## 核心概念

### `FogOfWarCamera`

给负责迷雾渲染的 2D 相机打上标记：

```rust
commands.spawn((Camera2d, FogOfWarCamera));
```

如果你希望相机 / 玩家本身也能开视野，可以同时挂上 `VisionSource`。

### `VisionSource`

内置常用形状构造器：

```rust
commands.spawn((Transform::default(), VisionSource::circle(120.0)));
commands.spawn((Transform::default(), VisionSource::square(120.0)));
commands.spawn((
    Transform::default(),
    VisionSource::cone(180.0, 0.0, std::f32::consts::FRAC_PI_2),
));
```

### `Capturable`

给需要“被发现后才显示”的实体添加 `Capturable`。

## 重置迷雾

发送 `ResetFogOfWar` 即可清空探索状态并重建迷雾纹理，不需要重建整张地图：

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

## 通过 Bevy message 做持久化

运行时保存 / 加载 API 基于 Bevy message 和原始字节数据；面向压缩文件的辅助能力在 `persistence_utils` 中。

### 保存

```rust
use bevy::prelude::*;
use bevy_fog_of_war::prelude::*;

fn request_save(mut save: MessageWriter<SaveFogOfWarRequest>) {
    save.write(SaveFogOfWarRequest {
        include_texture_data: true,
        format: None, // 自动选择当前启用的最佳 SerializationFormat
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

### 加载

```rust
use bevy::prelude::*;
use bevy_fog_of_war::prelude::*;

fn request_load(mut load: MessageWriter<LoadFogOfWarRequest>) {
    if let Ok(data) = std::fs::read("fog_save.bincode") {
        load.write(LoadFogOfWarRequest {
            data,
            format: None, // 从字节内容自动检测
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

## 持久化文件辅助工具

如果你想用基于扩展名的保存 / 加载，或者压缩文件格式，可以使用 `bevy_fog_of_war::persistence_utils`：

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

启用对应 feature 后，还可以使用 `FileFormat::BincodeZstd`、`FileFormat::MessagePackLz4` 等压缩变体。

## 示例

```bash
cargo run --example simple_2d
cargo run --example playground
```

- `simple_2d`：最小场景 + 保存 / 加载演示。
- `playground`：交互式示例，包含相机控制、实时设置文本、重置流程和持久化。

## API 文档

- Crate 文档：<https://docs.rs/bevy_fog_of_war>
- 示例源码：[`examples/`](examples/)

## 许可证

你可以任选其一：

- Apache License, Version 2.0（[LICENSE-APACHE](LICENSE-APACHE)）
- MIT license（[LICENSE-MIT](LICENSE-MIT)）

## 贡献

除非你明确声明，否则你有意提交到本项目中的任何贡献都将按上述双许可证授权，不附加额外条款。
