# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

这是一个 Bevy 游戏引擎的2D战争迷雾插件。该插件使用基于 chunk 的系统和 GPU 计算着色器来高效处理大型地图。

Key features:
- Chunk-based fog system (256x256 units per chunk by default)
- GPU compute shaders for performance
- Multiple vision shapes (circle, cone, square)
- Explored area tracking with snapshot system
- Dynamic CPU/GPU memory management

## Development Commands

```bash
# Build
cargo build

# Run examples
cargo run --example simple_2d
cargo run --example playground

# Code quality checks (run these before committing)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features
bevy_lint --workspace --all-targets --all-features  # if installed

# Documentation
cargo doc --workspace --all-features --document-private-items --no-deps

# Tests
cargo nextest run --workspace --all-targets --no-fail-fast --no-tests=warn
cargo test --workspace --doc --no-fail-fast
```

## Architecture

### Core Plugin Structure

The codebase follows Bevy's ECS pattern with clear separation of concerns:

1. **Main Plugin** (`src/lib.rs`): `FogOfWarPlugin` manages system execution order via `SystemSet`
2. **Components** (`src/components.rs`): 
   - `VisionSource`: Entities that reveal fog
   - `Capturable`: Entities that become visible when discovered
   - `FogOfWarCamera`: Marks the camera for fog rendering
3. **Render Pipeline** (`src/render/`): GPU compute shaders and texture management
4. **Memory Management** (`src/managers.rs`, `src/data_transfer.rs`): Intelligent CPU/GPU transfer

### Chunk System Design

The world is divided into chunks for efficient processing:
- Each chunk has fog and snapshot texture layers
- Chunks dynamically load/unload based on visibility
- Event-driven memory transfers between CPU and GPU

### Key Patterns

1. **Event-Driven Communication**: Systems communicate via Bevy events
2. **Resource Configuration**: `FogMapSettings` controls all behavior
3. **GPU-First Design**: Heavy computation happens in WGSL shaders
4. **Lazy Loading**: Chunks only exist when needed

## Important Notes

- 代码注释使用中英双语
- Git commits should be in English with conventional format
- Currently no test coverage - examples serve as manual tests
- Some unused function warnings exist that need cleanup
- Uses latest Bevy version (0.16)

## Common Tasks

### Adding a New Vision Shape
1. Update `VisionShape` enum in `src/components.rs`
2. Implement shape logic in compute shader (`src/render/compute_shader.wgsl`)
3. Add constructor method to `VisionSource`

### Debugging Fog Rendering
1. Check `FogOfWarCamera` is attached to camera
2. Verify `FogMapSettings` resource exists
3. Use `bevy::log` for system execution order issues
4. Check GPU buffer transfers in `data_transfer.rs`

### Performance Optimization
- Adjust `chunk_size` in `FogMapSettings` for your map scale
- Reduce `texture_resolution_per_chunk` for lower GPU memory usage
- Monitor `MemoryTransferEvent` frequency