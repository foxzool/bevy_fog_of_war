# Bevy 战争迷雾

中文 | [English](README.md)

![截屏2025-05-23 19 23 05](https://github.com/user-attachments/assets/d8821536-7c91-4527-9425-c64ee5252b20)

[![CI](https://github.com/foxzool/bevy_fog_of_war/workflows/CI/badge.svg)](https://github.com/foxzool/bevy_fog_of_war/actions)
[![Crates.io](https://img.shields.io/crates/v/bevy_fog_of_war)](https://crates.io/crates/bevy_fog_of_war)
[![Downloads](https://img.shields.io/crates/d/bevy_fog_of_war)](https://crates.io/crates/bevy_fog_of_war)
[![Documentation](https://docs.rs/bevy_fog_of_war/badge.svg)](https://docs.rs/bevy_fog_of_war)
[![MIT/Apache 2.0](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/Seldom-SE/seldom_pixel#license)

一个为 Bevy 游戏引擎实现的战争迷雾系统。这个库提供了一个简单的方法来为你的 2D 游戏添加战争迷雾效果，支持多个光源、平滑过渡和已探索区域跟踪。

## 特性

- 2D 战争迷雾，支持平滑过渡和可自定义的颜色
- 支持多个动态视野源，具有各种形状
- 已探索区域跟踪，可选择已探索区域的显示方式
- 基于区块的地图处理，高效更新，适合大型地图
- 快照系统，用于持久化已探索的迷雾数据
- 原子化迷雾重置功能，支持成功/失败事件通知
- 通过 `FogMapSettings` 资源高度可配置
- 使用 WGSL 计算着色器的高效 GPU 实现

## 使用方法

要在你的项目中使用 `bevy_fog_of_war`，请按照以下步骤操作：

（你可以通过 `use bevy_fog_of_war::prelude::*;` 导入大部分常用项目）

1. **将插件添加到你的应用程序：**

   将 `FogOfWarPlugin` 添加到你的 Bevy `App`：

   ```rust
   use bevy::prelude::*;
   use bevy_fog_of_war::FogOfWarPlugin;

   fn main() {
       App::new()
           .add_plugins(DefaultPlugins)
           .add_plugins(FogOfWarPlugin) // 添加战争迷雾插件
           // ... 其他设置 ...
           .run();
   }
   ```

2. **将 `FogOfWarCamera` 添加到你的摄像机：**

   插件需要知道哪个摄像机用于战争迷雾效果。将 `FogOfWarCamera` 组件添加到你的主 2D 摄像机实体。

   ```rust
   use bevy::prelude::*;
   use bevy_fog_of_war::prelude::FogOfWarCamera;

   fn setup(mut commands: Commands) {
       commands.spawn((Camera2d, FogOfWarCamera));
   }
   ```

3. **将 `VisionSource` 添加到实体：**

   需要揭示地图的实体需要一个 `VisionSource` 组件。你可以为视野区域创建不同的形状，如圆形、方形或锥形。

   ```rust
   use bevy::prelude::*;
   use bevy_fog_of_war::prelude::VisionSource;

   fn setup_entities(mut commands: Commands) {
       // 生成一个具有圆形视野源的实体
       commands.spawn((Transform::from_xyz(0.0, 0.0, 0.0), VisionSource::circle(200.0)));

       // 生成另一个具有方形视野源的实体
       commands.spawn((Transform::from_xyz(100.0, 50.0, 0.0), VisionSource::square(150.0)));
   }
   ```

4. **（可选）将 `Capturable` 添加到实体：**

   如果你有只有在 `VisionSource` 与其重叠时才应该变得可见的实体（根据你的迷雾设置，一旦被发现就保持可见），请为它们添加
   `Capturable` 组件。

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
           Capturable, // 这个实体将被视野源揭示
       ));
   }
   ```

5. **自定义 `FogMapSettings`（可选）：**

   你可以通过插入 `FogMapSettings` 资源来自定义战争迷雾行为。以下是如何配置它的示例：

   ```rust
   use bevy::prelude::*;
   use bevy_fog_of_war::prelude::*;
   use bevy::render::render_resource::TextureFormat;

   fn setup_fog_settings(mut commands: Commands) {
       commands.insert_resource(FogMapSettings {
           enabled: true,  // 启用/禁用战争迷雾效果
           chunk_size: UVec2::new(256, 256),  // 每个区块的世界单位大小
           texture_resolution_per_chunk: UVec2::new(512, 512),  // 每个区块的纹理分辨率
           fog_color_unexplored: Color::rgba(0.1, 0.1, 0.1, 0.9),  // 未探索区域的颜色
           fog_color_explored: Color::rgba(0.3, 0.3, 0.3, 0.5),   // 已探索但不可见区域的颜色
           vision_clear_color: Color::NONE,  // 可见区域的清除颜色（通常是透明的）
           fog_texture_format: TextureFormat::R8Unorm,  // 迷雾纹理格式
           snapshot_texture_format: TextureFormat::R8Unorm  // 快照纹理格式
       });
   }
   ```

   然后将系统添加到你的应用程序：

   ```rust
   .add_systems(Startup, setup_fog_settings)
   ```

查看 [examples](examples/) 目录了解更详细的使用场景，包括动态视野源和不同的视野形状。

## 重置战争迷雾

你可以程序化地重置所有战争迷雾数据（已探索区域、可见性状态和纹理数据），而无需销毁实体或摄像机。这在更换场景或重启关卡时很有用：

```rust
use bevy::prelude::*;
use bevy_fog_of_war::prelude::*;

fn reset_on_keypress(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut reset_events: EventWriter<ResetFogOfWarEvent>,
) {
    if keyboard_input.just_pressed(KeyCode::KeyR) {
        // 重置所有战争迷雾数据
        reset_events.write(ResetFogOfWarEvent);
    }
}

// 将此系统添加到你的应用程序
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(FogOfWarPlugin)
        .add_systems(Update, reset_on_keypress)
        .run();
}
```

### 重置事件通知

插件提供事件来通知你重置操作何时完成：

```rust
use bevy::prelude::*;
use bevy_fog_of_war::prelude::*;

fn handle_fog_reset_events(
    mut success_events: EventReader<FogResetSuccessEvent>,
    mut failure_events: EventReader<FogResetFailedEvent>,
) {
    for event in success_events.read() {
        info!("✅ 迷雾重置成功完成！持续时间：{}ms，重置区块数：{}", 
              event.duration_ms, event.chunks_reset);
    }

    for event in failure_events.read() {
        error!("❌ 迷雾重置失败！持续时间：{}ms，错误：{}", 
               event.duration_ms, event.error);
    }
}

// 将事件处理器添加到你的应用程序
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(FogOfWarPlugin)
        .add_systems(Update, (reset_on_keypress, handle_fog_reset_events))
        .run();
}
```

重置功能：

- 清除所有已探索区域
- 将所有区块可见性状态重置为未探索
- 将纹理数据重置为初始状态
- 保留所有实体、摄像机和视野源
- 允许无缝场景转换
- 提供带有时间信息的成功/失败事件
- 包含失败时的自动回滚

查看 [`playground.rs`](examples/playground.rs) 和 [`simple_2d.rs`](examples/simple_2d.rs) 示例了解完整演示。

## 兼容性

| Bevy 版本 | 插件版本  |
|---------|-------|
| 0.16    | 0.2.0 |
| 0.15    | 0.1.0 |

## 许可证

本项目采用以下任一许可证：

* Apache 许可证 2.0 版（[LICENSE-APACHE](LICENSE-APACHE) 或 https://www.apache.org/licenses/LICENSE-2.0）
* MIT 许可证（[LICENSE-MIT](LICENSE-MIT) 或 https://opensource.org/licenses/MIT）

由你选择。

## 贡献

除非你明确声明，否则你有意提交给作品的任何贡献（如 Apache-2.0 许可证中定义的）都应按上述方式双重许可，不附加任何额外条款或条件。
