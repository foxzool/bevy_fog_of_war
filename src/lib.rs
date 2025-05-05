use self::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureUsages};

mod chunk;
mod components;
mod fog_2d;
pub mod prelude;
mod resources;
mod sync_texture;
mod vision;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
enum FogSystemSet {
    /// Update chunk states based on vision and camera
    /// 更新区块状态 (基于视野和相机)
    UpdateChunkState,
    /// Manage chunk entities (creation, activation)
    /// 管理区块实体 (创建, 激活)
    ManageEntities,
    /// Handle CPU <-> GPU memory transfer logic
    /// 处理 CPU <-> GPU 内存传输逻辑
    ManageMemory,
    /// Prepare data for GPU processing (runs before Render Graph execution)
    /// 为 GPU 处理准备数据 (在 Render Graph 执行前运行)
    PrepareGpuData,
}

pub struct FogOfWarPlugin;

impl Plugin for FogOfWarPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                FogSystemSet::UpdateChunkState,
                FogSystemSet::ManageEntities,
                FogSystemSet::ManageMemory,
            )
                .chain(), // Ensure they run in this order / 确保它们按此顺序运行
        );

        app.add_systems(Startup, setup_fog_resources);

        app.add_plugins(chunk::ChunkManagerPlugin)
            .add_plugins(vision::VisionComputePlugin)
            .add_plugins(fog_2d::Fog2DRenderPlugin)
            .add_plugins(sync_texture::GpuSyncTexturePlugin);
    }
}

fn setup_fog_resources(
    mut commands: Commands,
    settings: Res<FogMapSettings>,
    mut images: ResMut<Assets<Image>>,
) {
    // --- Create Texture Arrays ---
    // --- 创建 Texture Arrays ---
    let array_layers = 64; // Example layer count, adjust as needed / 示例层数，按需调整
    info!("Setting up Fog of War with {} layers.", array_layers);

    let fog_texture_size = Extent3d {
        width: settings.texture_resolution_per_chunk.x,
        height: settings.texture_resolution_per_chunk.y,
        depth_or_array_layers: array_layers,
    };
    let snapshot_texture_size = fog_texture_size;

    // Fog Texture: R8Unorm (0=visible, 1=unexplored)
    // 雾效纹理: R8Unorm (0=可见, 1=未探索)
    let fog_initial_data = vec![
        255u8;
        (fog_texture_size.width * fog_texture_size.height * fog_texture_size.depth_or_array_layers)
            as usize
    ];
    let mut fog_image = Image::new(
        fog_texture_size,
        TextureDimension::D2,
        fog_initial_data,
        settings.fog_texture_format,
        RenderAssetUsages::RENDER_WORLD,
    );
    fog_image.texture_descriptor.usage = TextureUsages::STORAGE_BINDING // For compute shader write / 用于 compute shader 写入
        | TextureUsages::TEXTURE_BINDING // For sampling in overlay shader / 用于在覆盖 shader 中采样
        | TextureUsages::COPY_DST // For CPU->GPU transfer / 用于 CPU->GPU 传输
        | TextureUsages::COPY_SRC; // For GPU->CPU transfer / 用于 GPU->CPU 传输

    // Snapshot Texture: Rgba8UnormSrgb (Stores last visible scene)
    // 快照纹理: Rgba8UnormSrgb (存储最后可见的场景)
    let snapshot_initial_data = vec![
        0u8;
        (snapshot_texture_size.width
            * snapshot_texture_size.height
            * snapshot_texture_size.depth_or_array_layers
            * 4) as usize
    ]; // 4 bytes per pixel for RGBA / RGBA 每像素 4 字节
    let mut snapshot_image = Image::new(
        snapshot_texture_size,
        TextureDimension::D2,
        snapshot_initial_data,
        settings.snapshot_texture_format,
        RenderAssetUsages::RENDER_WORLD,
    );
    snapshot_image.texture_descriptor.usage = TextureUsages::RENDER_ATTACHMENT // To render snapshots into / 用于渲染快照
        | TextureUsages::TEXTURE_BINDING // For sampling in overlay shader / 用于在覆盖 shader 中采样
        | TextureUsages::COPY_DST // For CPU->GPU transfer / 用于 CPU->GPU 传输
        | TextureUsages::COPY_SRC; // For GPU->CPU transfer / 用于 GPU->CPU 传输

    let fog_handle = images.add(fog_image);
    let snapshot_handle = images.add(snapshot_image);

    // Insert resources
    // 插入资源
    commands.insert_resource(FogTextureArray { handle: fog_handle });
    commands.insert_resource(SnapshotTextureArray {
        handle: snapshot_handle,
    });
    commands.insert_resource(TextureArrayManager::new(array_layers));

    info!("Fog of War resources initialized.");
}
