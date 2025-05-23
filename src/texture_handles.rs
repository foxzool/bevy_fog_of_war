use crate::prelude::*;
use bevy::asset::Handle;
use bevy::prelude::Image;
use bevy::prelude::Resource;
use bevy::reflect::Reflect;

/// 存储可见性数据的 TextureArray 资源句柄
/// Resource handle for the TextureArray storing visibility data
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)] // 注册为反射资源 / Register as a reflectable resource
pub struct VisibilityTextureArray {
    /// 图像资源的句柄 / Handle to the image asset
    pub handle: Handle<Image>,
}

/// 存储雾效数据的 TextureArray 资源句柄
/// Resource handle for the TextureArray storing fog data
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)] // 注册为反射资源 / Register as a reflectable resource
pub struct FogTextureArray {
    /// 图像资源的句柄 / Handle to the image asset
    pub handle: Handle<Image>,
}

/// 存储快照数据的 TextureArray 资源句柄
/// Resource handle for the TextureArray storing snapshot data
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)] // 注册为反射资源 / Register as a reflectable resource
pub struct SnapshotTextureArray {
    /// 图像资源的句柄 / Handle to the image asset
    pub handle: Handle<Image>,
}

/// 存储快照数据的 TextureArray 资源句柄
/// Resource handle for the TextureArray storing snapshot data
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub struct SnapshotTempTexture {
    /// 图像资源的句柄 / Handle to the image asset
    pub handle: Handle<Image>,
}
