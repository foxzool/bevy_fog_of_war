use crate::prelude::InCameraView;
use bevy_ecs::prelude::*;
use bevy_encase_derive::ShaderType;
use bevy_math::Vec2;
use bevy_reflect::Reflect;
use bevy_render::{
    Extract,
    prelude::ViewVisibility,
    render_resource::{Buffer, BufferInitDescriptor, BufferUsages},
    renderer::RenderDevice,
};
use bevy_render_macros::ExtractComponent;
use bevy_transform::prelude::*;
use bytemuck::{Pod, Zeroable};
