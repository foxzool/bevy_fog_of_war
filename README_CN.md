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
- 保存/加载功能，支持按角色或存档文件持久化迷雾数据
- 多种序列化格式：JSON、MessagePack、bincode，支持压缩
- 服务器友好的序列化格式，支持自动格式检测
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

## 序列化格式

插件支持多种序列化格式以获得最佳性能和兼容性。选择最适合您需求的格式：

### 可用格式

| 格式 | 大小 | 速度 | 兼容性 | 使用场景 |
|------|------|------|--------|----------|
| **JSON** | 最大 | 慢 | 通用 | 调试、Web API、人类可读 |
| **MessagePack** | 小 | 快 | 跨语言 | 网络传输、存储高效 |
| **bincode** | 最小* | 最快 | 仅Rust | 本地存档、性能关键 |

*结合压缩时

### 功能标志

在您的 `Cargo.toml` 中添加所需格式：

```toml
[dependencies]
bevy_fog_of_war = { version = "0.2.1", features = ["format-messagepack", "format-bincode"] }

# 或启用所有功能：
bevy_fog_of_war = { version = "0.2.1", features = ["all-formats"] }

# 压缩支持：
bevy_fog_of_war = { version = "0.2.1", features = ["all-formats", "all-compression"] }
```

### 性能比较

基于典型迷雾数据的基准测试：

```
格式                  | 文件大小  | 保存时间  | 加载时间
---------------------|----------|----------|----------
JSON                 | 100%     | 100%     | 100%
JSON + gzip          | 65%      | 120%     | 110%
MessagePack          | 45%      | 30%      | 40%
MessagePack + LZ4    | 35%      | 35%      | 45%
bincode              | 30%      | 15%      | 20%
bincode + Zstandard  | 25%      | 20%      | 25%
```

### 使用示例

#### 基本格式选择

```rust
use bevy_fog_of_war::prelude::*;

// 以不同格式保存
save_data_to_file(&fog_data, "save.json", FileFormat::Json)?;
save_data_to_file(&fog_data, "save.msgpack", FileFormat::MessagePack)?;
save_data_to_file(&fog_data, "save.bincode", FileFormat::Bincode)?;

// 自动格式检测加载
let data = load_data_from_file::<FogOfWarSaveData>("save.msgpack", None)?;
```

#### 压缩格式

```rust
// 高压缩存储
save_data_to_file(&fog_data, "save.msgpack.zst", FileFormat::MessagePackZstd)?;

// 快速压缩，适合频繁保存
save_data_to_file(&fog_data, "save.bincode.lz4", FileFormat::BincodeLz4)?;

// 压缩格式也支持自动检测
let data = load_data_from_file::<FogOfWarSaveData>("save.msgpack.zst", None)?;
```

#### 智能加载优先级

插件自动按最优顺序尝试格式：

```rust
// 尝试顺序：.bincode.zst → .msgpack.zst → .bincode → .msgpack → .json
let result = load_fog_data("character_save", None)?;
```

### 格式建议

- **开发阶段**：使用 JSON 进行调试和手动检查
- **生产本地**：使用 bincode 获得最快的本地存档
- **网络/API**：使用 MessagePack 进行高效传输
- **长期存储**：使用压缩格式（Zstandard 压缩比最高）
- **跨平台**：使用 MessagePack 与其他语言兼容

## 持久化

插件支持保存和加载雾效数据，允许你在游戏会话之间或按角色/存档文件持久化已探索的区域。

### 保存雾效数据

```rust
use bevy::prelude::*;
use bevy_fog_of_war::prelude::*;

fn save_fog_data(
    mut save_events: EventWriter<SaveFogOfWarRequest>,
) {
    // 请求保存雾效数据
    save_events.write(SaveFogOfWarRequest {
        include_texture_data: true, // 包含纹理数据以保存部分可见性
    });
}

fn handle_save_complete(
    mut events: EventReader<FogOfWarSaved>,
) {
    for event in events.read() {
        println!("保存了 {} 个区块", event.chunk_count);
        
        // 将JSON数据解析为结构体以便灵活保存
        let save_data: FogOfWarSaveData = serde_json::from_str(&event.data).unwrap();
        
        // 演示多种格式保存
        let formats = vec![
            (FileFormat::Json, "json"),
            (FileFormat::MessagePack, "msgpack"),
            (FileFormat::BincodeZstd, "bincode.zst"), // 压缩存储
        ];
        
        for (format, ext) in formats {
            let filename = format!("fog_save.{}", ext);
            if let Err(e) = save_data_to_file(&save_data, &filename, format) {
                eprintln!("保存 {} 失败: {}", filename, e);
            } else {
                println!("已保存到 {} ({:?})", filename, format);
            }
        }
    }
}
```

### 加载雾效数据

```rust
use bevy::prelude::*;
use bevy_fog_of_war::prelude::*;

fn load_fog_data(
    mut load_events: EventWriter<LoadFogOfWarRequest>,
) {
    // 选项1：自动格式检测加载
    match load_data_from_file::<FogOfWarSaveData>("player_save.msgpack", None) {
        Ok(save_data) => {
            let json_data = serde_json::to_string(&save_data).unwrap();
            load_events.write(LoadFogOfWarRequest {
                data: json_data,
            });
        }
        Err(e) => eprintln!("加载失败: {}", e),
    }
    
    // 选项2：按优先级尝试多种格式（最快的优先）
    let format_priorities = [
        ("player_save.bincode.zst", Some(FileFormat::BincodeZstd)),
        ("player_save.msgpack", Some(FileFormat::MessagePack)),
        ("player_save.json", Some(FileFormat::Json)),
    ];
    
    for (filename, format) in format_priorities {
        if std::path::Path::new(filename).exists() {
            match load_data_from_file::<FogOfWarSaveData>(filename, format) {
                Ok(save_data) => {
                    println!("从 '{}' 加载（自动检测格式）", filename);
                    let json_data = serde_json::to_string(&save_data).unwrap();
                    load_events.write(LoadFogOfWarRequest {
                        data: json_data,
                    });
                    break;
                }
                Err(e) => println!("加载 {} 失败: {}", filename, e),
            }
        }
    }
}

fn handle_load_complete(
    mut events: EventReader<FogOfWarLoaded>,
) {
    for event in events.read() {
        println!("加载了 {} 个区块", event.chunk_count);
        
        if !event.warnings.is_empty() {
            println!("警告: {:?}", event.warnings);
        }
    }
}
```

### 服务器集成

持久化系统设计用于与服务器端存储配合使用：

```rust
// 服务器集成示例
async fn save_to_server(fog_data: &str) {
    // 将雾效数据发送到你的游戏服务器
    let response = reqwest::Client::new()
        .post("https://api.yourgame.com/fog-of-war/save")
        .json(&serde_json::json!({
            "fog_data": fog_data,
        }))
        .send()
        .await
        .unwrap();
}

async fn load_from_server() -> String {
    // 从你的游戏服务器获取雾效数据
    let response = reqwest::Client::new()
        .get("https://api.yourgame.com/fog-of-war")
        .send()
        .await
        .unwrap();
    
    response.text().await.unwrap()
}
```

查看 [`persistence.rs`](examples/persistence.rs) 示例了解保存和加载雾效数据的完整演示。

## 兼容性

| Bevy 版本 | 插件版本  |
|---------|-------|
| 0.16    | 0.2.1 |
| 0.15    | 0.1.0 |

## 许可证

本项目采用以下任一许可证：

* Apache 许可证 2.0 版（[LICENSE-APACHE](LICENSE-APACHE) 或 https://www.apache.org/licenses/LICENSE-2.0）
* MIT 许可证（[LICENSE-MIT](LICENSE-MIT) 或 https://opensource.org/licenses/MIT）

由你选择。

## 贡献

除非你明确声明，否则你有意提交给作品的任何贡献（如 Apache-2.0 许可证中定义的）都应按上述方式双重许可，不附加任何额外条款或条件。
