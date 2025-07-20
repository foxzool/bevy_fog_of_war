//! Convenient re-exports for the bevy_fog_of_war crate.
//! bevy_fog_of_war crate 的便捷重新导出
//!
//! This prelude module provides convenient access to the most commonly used types,
//! traits, and functions from the fog of war library. Import this module to get
//! quick access to essential functionality without needing to import individual modules.
//!
//! # Usage
//! Add this to your imports to access all commonly used fog of war types:
//! ```rust
//! use bevy_fog_of_war::prelude::*;
//! ```
//!
//! # Exported Categories
//!
//! ## Core Plugin and Components
//! - **FogOfWarPlugin**: Main plugin for fog of war functionality
//! - **Components**: VisionSource, Capturable, FogOfWarCamera, etc.
//! - **Snapshot System**: SnapshotPlugin, SnapshotCamera, Capturable, etc.
//!
//! ## Configuration and Settings
//! - **FogMapSettings**: Primary configuration resource
//! - **Settings**: Coordinate conversion, chunk configuration, texture formats
//!
//! ## Memory Management
//! - **Managers**: ChunkEntityManager, ChunkStateCache, TextureArrayManager
//! - **Data Transfer**: GPU↔CPU memory transfer events and requests
//! - **Reset Operations**: FogResetError, ResetFogOfWar events
//!
//! ## Persistence System
//! - **Persistence Plugin**: FogOfWarPersistencePlugin for save/load functionality
//! - **Save/Load Events**: SaveFogOfWarRequest, LoadFogOfWarRequest, completion events
//! - **Data Formats**: SerializationFormat, FogOfWarSaveData structures
//! - **Utilities**: File format handling, compression, size calculation
//!
//! ## Texture Resources
//! - **Texture Arrays**: VisibilityTextureArray, FogTextureArray, SnapshotTextureArray
//! - **Temporary Textures**: SnapshotTempTexture for capture operations
//!
//! # Performance Considerations
//! - **Import Cost**: Prelude imports are zero-cost at runtime
//! - **Compilation**: May increase compile time due to broader symbol visibility
//! - **Namespace**: Brings many symbols into scope, consider selective imports for large projects
//!
//! # Design Pattern
//! This follows Rust ecosystem conventions where crates provide a prelude module
//! for convenient access to commonly used items. Similar to `std::prelude` or
//! `bevy::prelude`, this reduces boilerplate imports for typical usage patterns.
//!
//! # Example Integration
//! ```rust
//! use bevy::prelude::*;
//! use bevy_fog_of_war::prelude::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugins(FogOfWarPlugin)
//!         .run();
//! }
//!
//! // All fog of war types available without additional imports
//! fn setup_fog_system(mut commands: Commands, mut settings: ResMut<FogMapSettings>) {
//!     // Use VisionSource, FogOfWarCamera, etc. directly
//! }
//! ```

// Data Transfer and Memory Management
// 数据传输和内存管理
pub use crate::data_transfer::{
    ChunkCpuDataUploaded, ChunkGpuDataReady, CpuToGpuCopyRequest, CpuToGpuCopyRequests,
    FogResetError, FogResetFailed, FogResetSuccess, FogResetSync, GpuToCpuCopyRequest,
    GpuToCpuCopyRequests, ResetCheckpoint, ResetFogOfWar, ResetSyncState, TextureSizeCalculator,
    TextureSizeInfo,
};

// Entity and Resource Managers
// 实体和资源管理器
pub use crate::managers::*;

// Persistence System Components
// 持久化系统组件
pub use crate::persistence::{
    FogOfWarLoaded, FogOfWarPersistencePlugin, FogOfWarSaveData, FogOfWarSaved,
    LoadFogOfWarRequest, PersistenceError, SaveFogOfWarRequest, SerializationFormat,
};

// Persistence Utility Functions
// 持久化实用功能函数
pub use crate::persistence_utils::{
    FileFormat, get_file_size_info, load_data_from_file, load_fog_data, load_from_file,
    save_data_to_file, save_fog_data, save_to_file,
};

// Configuration and Settings
// 配置和设置
pub use crate::settings::*;

// GPU Texture Resource Handles
// GPU纹理资源句柄
pub use crate::texture_handles::*;

// Core Plugin, Components, and Snapshot System
// 核心插件、组件和快照系统
pub use crate::{FogOfWarPlugin, components::*, snapshot::*};

// Internal Bevy re-exports for crate use
// 供 crate 内部使用的 Bevy 重新导出
pub(crate) use bevy::prelude::*;
