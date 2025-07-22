use crate::prelude::*;
use bevy_asset::Handle;
use bevy_image::Image;
use bevy_reflect::Reflect;

/// GPU texture array handle for real-time visibility data across fog chunks.
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)] // 注册为反射资源 / Register as a reflectable resource
pub struct VisibilityTextureArray {
    /// Handle to 3D texture array where each layer stores one chunk's visibility data.
    pub handle: Handle<Image>,
}

/// GPU texture array handle for persistent fog exploration data.
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)] // 注册为反射资源 / Register as a reflectable resource
pub struct FogTextureArray {
    /// Handle to 3D texture array storing cumulative exploration history per chunk.
    pub handle: Handle<Image>,
}

/// GPU texture array handle for visual snapshots of explored areas.
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)] // 注册为反射资源 / Register as a reflectable resource
pub struct SnapshotTextureArray {
    /// Handle to 3D texture array where each layer stores a snapshot of explored entities.
    pub handle: Handle<Image>,
}

/// Temporary texture handle used as intermediate render target for snapshot capture.
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub struct SnapshotTempTexture {
    /// Handle to temporary 2D texture used as intermediate render target for snapshots.
    pub handle: Handle<Image>,
}
