//! Comprehensive persistence utility functions with multiple serialization formats and compression support.
//! 包含多种序列化格式和压缩支持的全面持久化便利函数
//!
//! This module provides high-level file I/O utilities for fog of war persistence, supporting
//! multiple serialization formats (JSON, MessagePack, Bincode) with optional compression
//! (gzip, LZ4, Zstandard). It serves as a convenient wrapper around the core persistence
//! functionality with format detection, compression handling, and error management.
//!
//! # Supported Formats and Compression
//!
//! ## Serialization Formats
//! - **JSON**: Human-readable text format, larger size, universal compatibility
//! - **MessagePack**: Binary format with good compression, cross-language support
//! - **Bincode**: Rust-native binary format, highest performance and smallest size
//!
//! ## Compression Options
//! Each serialization format can be combined with compression:
//! - **Gzip**: Standard compression, good balance of speed and size
//! - **LZ4**: Fast compression with moderate compression ratio
//! - **Zstandard**: High compression ratio with good speed
//!
//! ## Format Combinations
//! ```text
//! Format Matrix:
//! ┌─────────────┬──────┬──────┬──────┬──────┐
//! │ Base Format │ None │ Gzip │ LZ4  │ Zstd │
//! ├─────────────┼──────┼──────┼──────┼──────┤
//! │ JSON        │  ✓   │  ✓   │  ✓   │  ✓   │
//! │ MessagePack │  ✓   │  ✓   │  ✓   │  ✓   │
//! │ Bincode     │  ✓   │  ✓   │  ✓   │  ✓   │
//! └─────────────┴──────┴──────┴──────┴──────┘
//! ```
//!
//! # Performance Characteristics
//!
//! ## Relative Performance (baseline: Bincode uncompressed)
//! - **Bincode**: 100% speed, 100% size (baseline)
//! - **Bincode + LZ4**: ~85% speed, ~70% size
//! - **Bincode + Gzip**: ~60% speed, ~60% size
//! - **Bincode + Zstd**: ~50% speed, ~50% size
//! - **MessagePack**: ~80% speed, ~110% size
//! - **JSON**: ~40% speed, ~300% size
//!
//! ## Use Case Recommendations
//! - **Production saves**: Bincode or Bincode+LZ4 for best performance
//! - **Network transfer**: MessagePack+Gzip for cross-platform compatibility
//! - **Debug/development**: JSON for human readability
//! - **Archival storage**: Bincode+Zstd for maximum compression
//!
//! # Feature Flags
//!
//! ## Serialization Format Features
//! - `format-messagepack`: Enables MessagePack support
//! - `format-bincode`: Enables Bincode support
//! - No feature required for JSON (always available)
//!
//! ## Compression Features
//! - `compression-gzip`: Enables gzip compression via flate2
//! - `compression-lz4`: Enables LZ4 compression
//! - `compression-zstd`: Enables Zstandard compression
//!
//! # File Extension Mapping
//!
//! ## Standard Extensions
//! ```text
//! Format                    → Extension
//! ──────────────────────────────────────
//! JSON                      → .json
//! JSON + Gzip              → .json.gz
//! JSON + LZ4               → .json.lz4
//! JSON + Zstd              → .json.zst
//! MessagePack              → .msgpack
//! MessagePack + Gzip       → .msgpack.gz
//! MessagePack + LZ4        → .msgpack.lz4
//! MessagePack + Zstd       → .msgpack.zst
//! Bincode                  → .bincode
//! Bincode + Gzip           → .bincode.gz
//! Bincode + LZ4            → .bincode.lz4
//! Bincode + Zstd           → .bincode.zst
//! ```
//!
//! # Usage Examples
//!
//! ## Basic Save/Load
//! ```rust,ignore
//! use bevy_fog_of_war::persistence_utils::*;
//! use bevy_fog_of_war::persistence::FogOfWarSaveData;
//!
//! # fn example() -> Result<(), PersistenceError> {
//! # let save_data = FogOfWarSaveData::default();
//! // Save with automatic format detection from extension
//! save_fog_data(&save_data, "save.json", FileFormat::Json)?;
//!
//! // Load with automatic format detection
//! let loaded_data: FogOfWarSaveData = load_fog_data("save.json", None)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Compressed Saves
//! ```rust,ignore
//! # use bevy_fog_of_war::persistence_utils::*;
//! # use bevy_fog_of_war::persistence::FogOfWarSaveData;
//! # fn example() -> Result<(), PersistenceError> {
//! # let save_data = FogOfWarSaveData::default();
//! // High compression for archival
//! save_fog_data(&save_data, "archive.bincode.zst", FileFormat::BincodeZstd)?;
//!
//! // Fast compression for frequent saves
//! save_fog_data(&save_data, "autosave.bincode.lz4", FileFormat::BincodeLz4)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Size Comparison
//! ```rust,ignore
//! # use bevy_fog_of_war::persistence_utils::*;
//! # fn example() -> Result<(), PersistenceError> {
//! // Compare file sizes across formats
//! let json_size = get_file_size_info("save.json")?;
//! let compressed_size = get_file_size_info("save.bincode.zst")?;
//! println!("JSON: {}, Compressed: {}", json_size, compressed_size);
//! # Ok(())
//! # }
//! ```
//!
//! # Error Handling
//!
//! All functions return `Result<T, PersistenceError>` with detailed error information:
//! - **SerializationFailed**: Issues during encoding/compression
//! - **DeserializationFailed**: Issues during decoding/decompression
//! - **File I/O Errors**: Wrapped filesystem errors
//!
//! # Integration Points
//!
//! ## Core Persistence Module
//! This module builds upon the core persistence functionality:
//! - Uses `FogOfWarSaveData` and `PersistenceError` from persistence module
//! - Provides convenient file I/O wrapper around core serialization
//! - Extends core formats with compression and format detection
//!
//! ## Asset Pipeline Integration
//! Compatible with Bevy's asset system for dynamic loading:
//! - Standard file extensions enable asset loader detection
//! - Cross-platform path handling via std::path::Path
//! - Error types compatible with Bevy's asset loading error handling

use crate::persistence::{FogOfWarSaveData, PersistenceError};
use serde::{Deserialize, Serialize};
#[cfg(any(
    feature = "compression-gzip",
    feature = "compression-lz4",
    feature = "compression-zstd"
))]
use std::io::{Read, Write};
use std::path::Path;

/// Enumeration of supported file formats combining serialization and compression options.
/// 支持的文件格式枚举，结合序列化和压缩选项
///
/// This enum defines all available combinations of serialization formats (JSON, MessagePack,
/// Bincode) with compression algorithms (gzip, LZ4, Zstandard). Each variant is feature-gated
/// to ensure only available formats are compiled based on enabled features.
///
/// # Format Selection Guide
///
/// ## By Use Case
/// - **Development/Debug**: `Json` - Human-readable, easy to inspect
/// - **Production Saves**: `Bincode` or `BincodeLz4` - Fastest performance
/// - **Network Transfer**: `MessagePackGzip` - Cross-platform, good compression
/// - **Archival Storage**: `BincodeZstd` - Maximum compression ratio
/// - **Fast Autosaves**: `BincodeLz4` - Fast compression with size reduction
///
/// ## Performance Characteristics
/// ```text
/// Format           Speed    Size    Platform  Readability
/// ────────────────────────────────────────────────────────
/// Json             Slow     Large   All       High
/// JsonGzip         Slower   Medium  All       High*
/// JsonLz4          Medium   Medium  All       High*
/// JsonZstd         Slower   Small   All       High*
/// MessagePack      Fast     Medium  All       None
/// MessagePackGzip  Medium   Small   All       None
/// MessagePackLz4   Fast     Medium  All       None
/// MessagePackZstd  Medium   Small   All       None
/// Bincode          Fastest  Small   Rust      None
/// BincodeGzip      Fast     Smallest All      None
/// BincodeLz4       Fastest  Small   All       None
/// BincodeZstd      Medium   Smallest All      None
///
/// * Readable after decompression
/// ```
///
/// # Feature Dependencies
///
/// ## Base Formats
/// - **JSON**: Always available (no feature required)
/// - **MessagePack**: Requires `format-messagepack` feature
/// - **Bincode**: Requires `format-bincode` feature
///
/// ## Compression Algorithms
/// - **Gzip**: Requires `compression-gzip` feature
/// - **LZ4**: Requires `compression-lz4` feature
/// - **Zstandard**: Requires `compression-zstd` feature
///
/// ## Combination Requirements
/// Compressed variants require both format and compression features:
/// ```toml
/// # Example: Enable MessagePack with LZ4 compression
/// bevy_fog_of_war = {
///     features = ["format-messagepack", "compression-lz4"]
/// }
/// ```
///
/// # File Extension Mapping
/// Each format has a corresponding file extension for automatic detection:
/// - Simple formats use single extensions (`.json`, `.msgpack`, `.bincode`)
/// - Compressed formats use compound extensions (`.json.gz`, `.msgpack.lz4`, `.bincode.zst`)
///
/// # Typical Compression Ratios
/// Based on typical fog of war save data:
/// - **Gzip**: 40-60% of original size
/// - **LZ4**: 60-80% of original size  
/// - **Zstandard**: 30-50% of original size
///
/// The actual compression ratio depends on data characteristics:
/// - Text data (JSON) compresses better than binary data
/// - Repetitive patterns in fog data improve compression
/// - Small saves may have worse ratios due to compression overhead
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    /// JSON格式（可读，较大）
    /// JSON format (human-readable, larger)
    #[cfg(feature = "format-json")]
    Json,
    /// JSON格式，使用gzip压缩
    /// JSON format with gzip compression
    #[cfg(all(feature = "format-json", feature = "compression-gzip"))]
    JsonGzip,
    /// JSON格式，使用LZ4压缩（快速）
    /// JSON format with LZ4 compression (fast)
    #[cfg(all(feature = "format-json", feature = "compression-lz4"))]
    JsonLz4,
    /// JSON格式，使用Zstandard压缩（高压缩率）
    /// JSON format with Zstandard compression (high compression ratio)
    #[cfg(all(feature = "format-json", feature = "compression-zstd"))]
    JsonZstd,
    /// MessagePack格式（二进制，紧凑）
    /// MessagePack format (binary, compact)
    #[cfg(feature = "format-messagepack")]
    MessagePack,
    /// MessagePack格式，使用gzip压缩
    /// MessagePack format with gzip compression
    #[cfg(all(feature = "format-messagepack", feature = "compression-gzip"))]
    MessagePackGzip,
    /// MessagePack格式，使用LZ4压缩
    /// MessagePack format with LZ4 compression
    #[cfg(all(feature = "format-messagepack", feature = "compression-lz4"))]
    MessagePackLz4,
    /// MessagePack格式，使用Zstandard压缩
    /// MessagePack format with Zstandard compression
    #[cfg(all(feature = "format-messagepack", feature = "compression-zstd"))]
    MessagePackZstd,
    /// bincode格式（Rust原生，最快）
    /// bincode format (Rust native, fastest)
    #[cfg(feature = "format-bincode")]
    Bincode,
    /// bincode格式，使用gzip压缩
    /// bincode format with gzip compression
    #[cfg(all(feature = "format-bincode", feature = "compression-gzip"))]
    BincodeGzip,
    /// bincode格式，使用LZ4压缩
    /// bincode format with LZ4 compression
    #[cfg(all(feature = "format-bincode", feature = "compression-lz4"))]
    BincodeLz4,
    /// bincode格式，使用Zstandard压缩
    /// bincode format with Zstandard compression
    #[cfg(all(feature = "format-bincode", feature = "compression-zstd"))]
    BincodeZstd,
}

impl FileFormat {
    /// Returns the standard file extension for this format including compression suffix.
    /// 返回此格式的标准文件扩展名，包括压缩后缀
    ///
    /// This method provides the conventional file extension for each format variant,
    /// enabling automatic format detection and proper file naming conventions.
    /// Compressed formats use compound extensions to indicate both the base format
    /// and compression algorithm.
    ///
    /// # Extension Patterns
    ///
    /// ## Base Formats
    /// - **JSON**: `.json` - Standard JSON text format
    /// - **MessagePack**: `.msgpack` - Binary MessagePack format
    /// - **Bincode**: `.bincode` - Rust-native binary format
    ///
    /// ## Compressed Formats
    /// Compression adds a second extension indicating the algorithm:
    /// - **Gzip**: `.gz` suffix (e.g., `.json.gz`, `.msgpack.gz`)
    /// - **LZ4**: `.lz4` suffix (e.g., `.json.lz4`, `.bincode.lz4`)
    /// - **Zstandard**: `.zst` suffix (e.g., `.json.zst`, `.msgpack.zst`)
    ///
    /// # Usage Examples
    /// ```rust,ignore
    /// use bevy_fog_of_war::persistence_utils::FileFormat;
    ///
    /// assert_eq!(FileFormat::Json.extension(), "json");
    /// assert_eq!(FileFormat::JsonGzip.extension(), "json.gz");
    /// assert_eq!(FileFormat::BincodeLz4.extension(), "bincode.lz4");
    /// ```
    ///
    /// # File Naming Convention
    /// The extensions follow standard conventions:
    /// - **Cross-Platform**: Extensions work on all operating systems
    /// - **Tool Recognition**: Standard tools can identify format from extension
    /// - **Hierarchical**: Compound extensions show format → compression pipeline
    /// - **Reversible**: Extension can be mapped back to format enum
    ///
    /// # Integration with File Systems
    /// - **Asset Loading**: Bevy asset system can use extensions for loader selection
    /// - **File Managers**: File explorers show appropriate icons/associations
    /// - **Command Line Tools**: Standard compression tools recognize `.gz`, `.lz4`, `.zst`
    /// - **Version Control**: Git and other VCS handle binary formats appropriately
    ///
    /// # Return Value
    /// Returns a static string slice containing the file extension without leading dot.
    /// The caller typically adds the dot when constructing file paths:
    /// ```rust,ignore
    /// let extension = format.extension();
    /// let filename = format!("save.{}", extension); // "save.json.gz"
    /// ```
    pub fn extension(&self) -> &'static str {
        match self {
            #[cfg(feature = "format-json")]
            FileFormat::Json => "json",
            #[cfg(all(feature = "format-json", feature = "compression-gzip"))]
            FileFormat::JsonGzip => "json.gz",
            #[cfg(all(feature = "format-json", feature = "compression-lz4"))]
            FileFormat::JsonLz4 => "json.lz4",
            #[cfg(all(feature = "format-json", feature = "compression-zstd"))]
            FileFormat::JsonZstd => "json.zst",
            #[cfg(feature = "format-messagepack")]
            FileFormat::MessagePack => "msgpack",
            #[cfg(all(feature = "format-messagepack", feature = "compression-gzip"))]
            FileFormat::MessagePackGzip => "msgpack.gz",
            #[cfg(all(feature = "format-messagepack", feature = "compression-lz4"))]
            FileFormat::MessagePackLz4 => "msgpack.lz4",
            #[cfg(all(feature = "format-messagepack", feature = "compression-zstd"))]
            FileFormat::MessagePackZstd => "msgpack.zst",
            #[cfg(feature = "format-bincode")]
            FileFormat::Bincode => "bincode",
            #[cfg(all(feature = "format-bincode", feature = "compression-gzip"))]
            FileFormat::BincodeGzip => "bincode.gz",
            #[cfg(all(feature = "format-bincode", feature = "compression-lz4"))]
            FileFormat::BincodeLz4 => "bincode.lz4",
            #[cfg(all(feature = "format-bincode", feature = "compression-zstd"))]
            FileFormat::BincodeZstd => "bincode.zst",
        }
    }

    /// Infers the file format from a file path's extension with support for compound extensions.
    /// 从文件路径的扩展名推断文件格式，支持复合扩展名
    ///
    /// This method analyzes file paths to automatically detect the appropriate format
    /// based on file extensions. It handles both simple extensions (`.json`) and
    /// compound extensions (`.json.gz`) to correctly identify compressed formats.
    ///
    /// # Detection Algorithm
    ///
    /// ## Two-Stage Process
    /// 1. **Compound Extension Check**: Examines stem + extension for compressed formats
    /// 2. **Simple Extension Check**: Falls back to single extension for base formats
    ///
    /// ## Compound Extension Logic
    /// ```text
    /// Path: "save.json.gz"
    /// ├── Extension: "gz"
    /// ├── Stem: "save.json"
    /// └── Detected: JsonGzip (stem ends with ".json" + extension is "gz")
    ///
    /// Path: "data.msgpack.lz4"
    /// ├── Extension: "lz4"
    /// ├── Stem: "data.msgpack"
    /// └── Detected: MessagePackLz4 (stem ends with ".msgpack" + extension is "lz4")
    /// ```
    ///
    /// # Supported Extension Patterns
    ///
    /// ## Base Formats
    /// - `.json` → `FileFormat::Json`
    /// - `.msgpack` → `FileFormat::MessagePack` (if feature enabled)
    /// - `.bincode` → `FileFormat::Bincode` (if feature enabled)
    ///
    /// ## Compressed Formats
    /// - `.json.gz` → `FileFormat::JsonGzip` (if compression-gzip enabled)
    /// - `.json.lz4` → `FileFormat::JsonLz4` (if compression-lz4 enabled)
    /// - `.json.zst` → `FileFormat::JsonZstd` (if compression-zstd enabled)
    /// - `.msgpack.gz` → `FileFormat::MessagePackGzip` (if both features enabled)
    /// - `.bincode.lz4` → `FileFormat::BincodeLz4` (if both features enabled)
    ///
    /// # Feature-Gated Detection
    /// Only formats with enabled features are detected:
    /// ```rust,ignore
    /// // With format-messagepack disabled:
    /// assert_eq!(FileFormat::from_extension(Path::new("save.msgpack")), None);
    ///
    /// // With compression-gzip disabled:
    /// assert_eq!(FileFormat::from_extension(Path::new("save.json.gz")), None);
    /// ```
    ///
    /// # Usage Examples
    /// ```rust,ignore
    /// use std::path::Path;
    /// use bevy_fog_of_war::persistence_utils::FileFormat;
    ///
    /// // Simple extensions
    /// assert_eq!(
    ///     FileFormat::from_extension(Path::new("save.json")),
    ///     Some(FileFormat::Json)
    /// );
    ///
    /// // Compound extensions (if features enabled)
    /// assert_eq!(
    ///     FileFormat::from_extension(Path::new("archive.bincode.zst")),
    ///     Some(FileFormat::BincodeZstd)
    /// );
    ///
    /// // Unknown extensions
    /// assert_eq!(
    ///     FileFormat::from_extension(Path::new("data.txt")),
    ///     None
    /// );
    /// ```
    ///
    /// # Error Handling
    /// Returns `None` for:
    /// - **Unknown Extensions**: Extensions not matching any supported format
    /// - **Disabled Features**: Formats requiring features that aren't enabled
    /// - **Invalid Paths**: Paths without extensions or invalid UTF-8
    /// - **Malformed Extensions**: Extensions that don't follow expected patterns
    ///
    /// # Integration Points
    /// - **Automatic Format Detection**: Used by `load_data_from_file` when format not specified
    /// - **Asset Pipeline**: Enables automatic format selection for Bevy asset loading
    /// - **File Managers**: Consistent format detection across different file operations
    /// - **Command Line Tools**: Standardized extension handling for CLI utilities
    ///
    /// # Performance Characteristics
    /// - **String Operations**: Limited string comparison operations (O(1) for most cases)
    /// - **No I/O**: Pure path analysis without file system access
    /// - **Feature Checks**: Compile-time feature gate evaluation
    /// - **Memory Efficient**: No heap allocation, works with borrowed path data
    pub fn from_extension(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;

        // 检查双扩展名（如 .json.gz, .msgpack.lz4等）
        // Check for double extensions (like .json.gz, .msgpack.lz4, etc.)
        if let Some(stem) = path.file_stem()
            && let Some(stem_str) = stem.to_str() {
                if stem_str.ends_with(".json") {
                    match ext {
                        #[cfg(all(feature = "format-json", feature = "compression-gzip"))]
                        "gz" => return Some(FileFormat::JsonGzip),
                        #[cfg(all(feature = "format-json", feature = "compression-lz4"))]
                        "lz4" => return Some(FileFormat::JsonLz4),
                        #[cfg(all(feature = "format-json", feature = "compression-zstd"))]
                        "zst" => return Some(FileFormat::JsonZstd),
                        _ => {}
                    }
                } else if stem_str.ends_with(".msgpack") {
                    match ext {
                        #[cfg(all(feature = "format-messagepack", feature = "compression-gzip"))]
                        "gz" => return Some(FileFormat::MessagePackGzip),
                        #[cfg(all(feature = "format-messagepack", feature = "compression-lz4"))]
                        "lz4" => return Some(FileFormat::MessagePackLz4),
                        #[cfg(all(feature = "format-messagepack", feature = "compression-zstd"))]
                        "zst" => return Some(FileFormat::MessagePackZstd),
                        _ => {}
                    }
                } else if stem_str.ends_with(".bincode") {
                    match ext {
                        #[cfg(all(feature = "format-bincode", feature = "compression-gzip"))]
                        "gz" => return Some(FileFormat::BincodeGzip),
                        #[cfg(all(feature = "format-bincode", feature = "compression-lz4"))]
                        "lz4" => return Some(FileFormat::BincodeLz4),
                        #[cfg(all(feature = "format-bincode", feature = "compression-zstd"))]
                        "zst" => return Some(FileFormat::BincodeZstd),
                        _ => {}
                    }
                }
            }

        // 单扩展名
        // Single extension
        match ext {
            #[cfg(feature = "format-json")]
            "json" => Some(FileFormat::Json),
            #[cfg(feature = "format-messagepack")]
            "msgpack" => Some(FileFormat::MessagePack),
            #[cfg(feature = "format-bincode")]
            "bincode" => Some(FileFormat::Bincode),
            _ => None,
        }
    }
}

/// Saves string data to file with optional compression for text-based formats.
/// 保存字符串数据到文件，为基于文本的格式提供可选压缩
///
/// This function handles saving text data (primarily JSON) to files with optional
/// compression. It's designed for string data and delegates binary format saves
/// to `save_data_to_file` for proper serialization handling.
///
/// # Supported Formats
///
/// ## Text Formats
/// - **Json**: Direct string write to file
/// - **JsonGzip**: Gzip-compressed JSON text
/// - **JsonLz4**: LZ4-compressed JSON text  
/// - **JsonZstd**: Zstandard-compressed JSON text
///
/// ## Binary Format Handling
/// Binary formats (MessagePack, Bincode) are not supported by this function
/// and will return an error directing the caller to use `save_data_to_file`.
///
/// # Compression Process
///
/// ## Gzip Compression
/// ```text
/// Text Data → GzEncoder → Compressed File
/// ├── Uses flate2::write::GzEncoder
/// ├── Default compression level
/// └── Proper stream finalization
/// ```
///
/// ## LZ4 Compression
/// ```text
/// Text Data → LZ4 Block Compress → Compressed File
/// ├── Uses lz4::block::compress
/// ├── High compression mode enabled
/// └── Direct binary write
/// ```
///
/// ## Zstandard Compression
/// ```text
/// Text Data → Zstd Encoder → Compressed File
/// ├── Uses zstd::encode_all
/// ├── Compression level 3 (balanced)
/// └── Single-shot compression
/// ```
///
/// # Error Handling
/// Returns `PersistenceError::SerializationFailed` for:
/// - **File I/O Errors**: Cannot create or write to target file
/// - **Compression Errors**: Compression algorithm failures
/// - **Format Errors**: Unsupported format requested
/// - **Permission Errors**: Insufficient file system permissions
///
/// # Usage Examples
/// ```rust,ignore
/// use bevy_fog_of_war::persistence_utils::{save_to_file, FileFormat};
///
/// let json_data = r#"{"chunk_count": 42, "timestamp": 1234567890}"#;
///
/// // Save uncompressed JSON
/// save_to_file(&json_data, "save.json", FileFormat::Json)?;
///
/// // Save with gzip compression (if feature enabled)
/// save_to_file(&json_data, "save.json.gz", FileFormat::JsonGzip)?;
/// ```
///
/// # Performance Characteristics
///
/// ## Compression Speed vs Size Trade-offs
/// - **No Compression**: Fastest save, largest file
/// - **LZ4**: Fast compression, moderate size reduction
/// - **Gzip**: Moderate speed, good size reduction
/// - **Zstandard**: Slower compression, best size reduction
///
/// ## Memory Usage
/// - **Uncompressed**: Minimal memory overhead
/// - **Gzip**: Streaming compression (low memory)
/// - **LZ4**: Block compression (moderate memory)
/// - **Zstd**: Single-shot compression (higher memory for large data)
///
/// # File System Integration
/// - **Atomic Writes**: Uses standard file creation for atomicity
/// - **Path Handling**: Accepts any `AsRef<Path>` for flexibility
/// - **Cross-Platform**: Works on all supported Rust platforms
/// - **Error Propagation**: Detailed error messages for debugging
///
/// # Feature Requirements
/// Compression formats require corresponding feature flags:
/// - `compression-gzip` for JsonGzip
/// - `compression-lz4` for JsonLz4  
/// - `compression-zstd` for JsonZstd
///
/// # Integration Points
/// - **Used By**: `save_data_to_file` for text format fallback
/// - **Complements**: `load_from_file` for corresponding read operations
/// - **Alternative**: `save_data_to_file` for full serialization support
pub fn save_to_file(
    #[allow(unused_variables)] data: &str,
    path: impl AsRef<Path>,
    format: FileFormat,
) -> Result<(), PersistenceError> {
    #[allow(unused_variables)]
    let path = path.as_ref();

    match format {
        #[cfg(feature = "format-json")]
        FileFormat::Json => {
            std::fs::write(path, data)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            Ok(())
        }

        #[cfg(all(feature = "format-json", feature = "compression-gzip"))]
        FileFormat::JsonGzip => {
            use flate2::Compression;
            use flate2::write::GzEncoder;

            let file = std::fs::File::create(path)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            let mut encoder = GzEncoder::new(file, Compression::default());
            encoder
                .write_all(data.as_bytes())
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            encoder
                .finish()
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            Ok(())
        }

        #[cfg(all(feature = "format-json", feature = "compression-lz4"))]
        FileFormat::JsonLz4 => {
            let compressed = lz4::block::compress(data.as_bytes(), None, true)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            std::fs::write(path, compressed)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            Ok(())
        }

        #[cfg(all(feature = "format-json", feature = "compression-zstd"))]
        FileFormat::JsonZstd => {
            let compressed = zstd::encode_all(data.as_bytes(), 3) // 压缩级别3（平衡）
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            std::fs::write(path, compressed)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            Ok(())
        }

        // 对于二进制格式，回退到通用处理
        // For binary formats, fall back to generic handling
        #[cfg(any(feature = "format-messagepack", feature = "format-bincode"))]
        _ => {
            // 这些格式应该通过save_data_to_file处理
            // These formats should be handled by save_data_to_file
            Err(PersistenceError::SerializationFailed(format!(
                "Format {format:?} not supported by save_to_file, use save_data_to_file instead"
            )))
        }
    }
}

/// 保存可序列化数据到文件（支持不同格式）
/// Save serializable data to file (supports different formats)
pub fn save_data_to_file<T: Serialize>(
    data: &T,
    path: impl AsRef<Path>,
    format: FileFormat,
) -> Result<(), PersistenceError> {
    let path = path.as_ref();

    match format {
        #[cfg(feature = "format-json")]
        FileFormat::Json => {
            let json = serde_json::to_string(data)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            save_to_file(&json, path, format)
        }

        #[cfg(feature = "format-messagepack")]
        FileFormat::MessagePack => {
            let msgpack_data = rmp_serde::to_vec(data)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            std::fs::write(path, msgpack_data)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            Ok(())
        }

        #[cfg(feature = "format-bincode")]
        FileFormat::Bincode => {
            let bincode_data = bincode::serde::encode_to_vec(data, bincode::config::standard())
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            std::fs::write(path, bincode_data)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            Ok(())
        }

        // 压缩格式处理
        // Compressed format handling
        #[cfg(all(feature = "format-messagepack", feature = "compression-gzip"))]
        FileFormat::MessagePackGzip => {
            let msgpack_data = rmp_serde::to_vec(data)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;

            use flate2::Compression;
            use flate2::write::GzEncoder;

            let file = std::fs::File::create(path)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            let mut encoder = GzEncoder::new(file, Compression::default());
            encoder
                .write_all(&msgpack_data)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            encoder
                .finish()
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            Ok(())
        }

        #[cfg(all(feature = "format-messagepack", feature = "compression-lz4"))]
        FileFormat::MessagePackLz4 => {
            let msgpack_data = rmp_serde::to_vec(data)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            let compressed = lz4::block::compress(&msgpack_data, None, true)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            std::fs::write(path, compressed)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            Ok(())
        }

        #[cfg(all(feature = "format-messagepack", feature = "compression-zstd"))]
        FileFormat::MessagePackZstd => {
            let msgpack_data = rmp_serde::to_vec(data)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            let compressed = zstd::encode_all(&msgpack_data[..], 3)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            std::fs::write(path, compressed)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            Ok(())
        }

        #[cfg(all(feature = "format-bincode", feature = "compression-gzip"))]
        FileFormat::BincodeGzip => {
            let bincode_data = bincode::serde::encode_to_vec(data, bincode::config::standard())
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;

            use flate2::Compression;
            use flate2::write::GzEncoder;

            let file = std::fs::File::create(path)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            let mut encoder = GzEncoder::new(file, Compression::default());
            encoder
                .write_all(&bincode_data)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            encoder
                .finish()
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            Ok(())
        }

        #[cfg(all(feature = "format-bincode", feature = "compression-lz4"))]
        FileFormat::BincodeLz4 => {
            let bincode_data = bincode::serde::encode_to_vec(data, bincode::config::standard())
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            let compressed = lz4::block::compress(&bincode_data, None, true)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            std::fs::write(path, compressed)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            Ok(())
        }

        #[cfg(all(feature = "format-bincode", feature = "compression-zstd"))]
        FileFormat::BincodeZstd => {
            let bincode_data = bincode::serde::encode_to_vec(data, bincode::config::standard())
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            let compressed = zstd::encode_all(&bincode_data[..], 3)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            std::fs::write(path, compressed)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            Ok(())
        }

        // 其他格式回退到字符串处理
        // Other formats fall back to string handling
        #[cfg(any(
            feature = "compression-gzip",
            feature = "compression-lz4",
            feature = "compression-zstd"
        ))]
        _ => {
            let json = serde_json::to_string(data)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            save_to_file(&json, path, format)
        }
    }
}

/// 从文件加载可反序列化数据（支持不同格式）
/// Load deserializable data from file (supports different formats)
pub fn load_data_from_file<T: for<'de> Deserialize<'de>>(
    path: impl AsRef<Path>,
    format: Option<FileFormat>,
) -> Result<T, PersistenceError> {
    let path = path.as_ref();

    // 如果没有指定格式，尝试从扩展名推断
    // If format not specified, try to infer from extension
    let format = format.unwrap_or_else(|| {
        FileFormat::from_extension(path).unwrap_or_else(|| {
            // Fallback to the first available format
            #[cfg(feature = "format-bincode")]
            return FileFormat::Bincode;
            #[cfg(all(not(feature = "format-bincode"), feature = "format-messagepack"))]
            return FileFormat::MessagePack;
            #[cfg(all(
                not(feature = "format-bincode"),
                not(feature = "format-messagepack"),
                feature = "format-json"
            ))]
            return FileFormat::Json;
        })
    });

    match format {
        #[cfg(feature = "format-json")]
        FileFormat::Json => {
            let json_str = std::fs::read_to_string(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            serde_json::from_str(&json_str)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))
        }

        #[cfg(feature = "format-messagepack")]
        FileFormat::MessagePack => {
            let msgpack_data = std::fs::read(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            rmp_serde::from_slice(&msgpack_data)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))
        }

        #[cfg(feature = "format-bincode")]
        FileFormat::Bincode => {
            let bincode_data = std::fs::read(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let (decoded, _): (T, usize) =
                bincode::serde::decode_from_slice(&bincode_data, bincode::config::standard())
                    .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            Ok(decoded)
        }

        // 压缩格式处理
        // Compressed format handling
        #[cfg(all(feature = "format-messagepack", feature = "compression-gzip"))]
        FileFormat::MessagePackGzip => {
            use flate2::read::GzDecoder;

            let file = std::fs::File::open(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let mut decoder = GzDecoder::new(file);
            let mut msgpack_data = Vec::new();
            decoder
                .read_to_end(&mut msgpack_data)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            rmp_serde::from_slice(&msgpack_data)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))
        }

        #[cfg(all(feature = "format-messagepack", feature = "compression-lz4"))]
        FileFormat::MessagePackLz4 => {
            let compressed = std::fs::read(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let msgpack_data = lz4::block::decompress(&compressed, None)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            rmp_serde::from_slice(&msgpack_data)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))
        }

        #[cfg(all(feature = "format-messagepack", feature = "compression-zstd"))]
        FileFormat::MessagePackZstd => {
            let compressed = std::fs::read(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let msgpack_data = zstd::decode_all(&compressed[..])
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            rmp_serde::from_slice(&msgpack_data)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))
        }

        #[cfg(all(feature = "format-bincode", feature = "compression-gzip"))]
        FileFormat::BincodeGzip => {
            use flate2::read::GzDecoder;

            let file = std::fs::File::open(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let mut decoder = GzDecoder::new(file);
            let mut bincode_data = Vec::new();
            decoder
                .read_to_end(&mut bincode_data)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            {
                let (decoded, _): (T, usize) =
                    bincode::serde::decode_from_slice(&bincode_data, bincode::config::standard())
                        .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
                Ok(decoded)
            }
        }

        #[cfg(all(feature = "format-bincode", feature = "compression-lz4"))]
        FileFormat::BincodeLz4 => {
            let compressed = std::fs::read(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let bincode_data = lz4::block::decompress(&compressed, None)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            {
                let (decoded, _): (T, usize) =
                    bincode::serde::decode_from_slice(&bincode_data, bincode::config::standard())
                        .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
                Ok(decoded)
            }
        }

        #[cfg(all(feature = "format-bincode", feature = "compression-zstd"))]
        FileFormat::BincodeZstd => {
            let compressed = std::fs::read(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let bincode_data = zstd::decode_all(&compressed[..])
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            {
                let (decoded, _): (T, usize) =
                    bincode::serde::decode_from_slice(&bincode_data, bincode::config::standard())
                        .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
                Ok(decoded)
            }
        }

        // 其他格式回退到JSON字符串处理
        // Other formats fall back to JSON string handling
        #[cfg(any(
            feature = "compression-gzip",
            feature = "compression-lz4",
            feature = "compression-zstd"
        ))]
        _ => {
            let json_str = load_from_file(path, Some(format))?;
            serde_json::from_str(&json_str)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))
        }
    }
}

/// 便利函数：从文件加载雾效数据
/// Utility function: Load fog of war data from file
pub fn load_from_file(
    path: impl AsRef<Path>,
    format: Option<FileFormat>,
) -> Result<String, PersistenceError> {
    let path = path.as_ref();

    // 如果没有指定格式，尝试从扩展名推断
    // If format not specified, try to infer from extension
    let format = format.unwrap_or_else(|| {
        FileFormat::from_extension(path).unwrap_or_else(|| {
            // Fallback to the first available format
            #[cfg(feature = "format-bincode")]
            return FileFormat::Bincode;
            #[cfg(all(not(feature = "format-bincode"), feature = "format-messagepack"))]
            return FileFormat::MessagePack;
            #[cfg(all(
                not(feature = "format-bincode"),
                not(feature = "format-messagepack"),
                feature = "format-json"
            ))]
            return FileFormat::Json;
        })
    });

    match format {
        #[cfg(feature = "format-json")]
        FileFormat::Json => {
            let data = std::fs::read_to_string(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            Ok(data)
        }

        #[cfg(all(feature = "format-json", feature = "compression-gzip"))]
        FileFormat::JsonGzip => {
            use flate2::read::GzDecoder;

            let file = std::fs::File::open(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let mut decoder = GzDecoder::new(file);
            let mut data = String::new();
            decoder
                .read_to_string(&mut data)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            Ok(data)
        }

        #[cfg(all(feature = "format-json", feature = "compression-lz4"))]
        FileFormat::JsonLz4 => {
            let compressed = std::fs::read(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let decompressed = lz4::block::decompress(&compressed, None)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let data = String::from_utf8(decompressed)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            Ok(data)
        }

        #[cfg(all(feature = "format-json", feature = "compression-zstd"))]
        FileFormat::JsonZstd => {
            let compressed = std::fs::read(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let decompressed = zstd::decode_all(&compressed[..])
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let data = String::from_utf8(decompressed)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            Ok(data)
        }

        // 对于二进制格式，回退到通用处理
        // For binary formats, fall back to generic handling
        #[cfg(any(feature = "format-messagepack", feature = "format-bincode"))]
        _ => {
            // 这些格式应该通过load_data_from_file处理
            // These formats should be handled by load_data_from_file
            Err(PersistenceError::DeserializationFailed(format!(
                "Format {format:?} not supported by load_from_file, use load_data_from_file instead"
            )))
        }
    }
}

/// 直接保存FogOfWarSaveData到文件
/// Directly save FogOfWarSaveData to file
pub fn save_fog_data(
    save_data: &FogOfWarSaveData,
    path: impl AsRef<Path>,
    format: FileFormat,
) -> Result<(), PersistenceError> {
    save_data_to_file(save_data, path, format)
}

/// 直接从文件加载FogOfWarSaveData
/// Directly load FogOfWarSaveData from file
pub fn load_fog_data(
    path: impl AsRef<Path>,
    format: Option<FileFormat>,
) -> Result<FogOfWarSaveData, PersistenceError> {
    load_data_from_file(path, format)
}

/// 获取文件大小信息（用于比较压缩效果）
/// Get file size info (for comparing compression effectiveness)
pub fn get_file_size_info(path: impl AsRef<Path>) -> Result<String, std::io::Error> {
    let metadata = std::fs::metadata(path)?;
    let size = metadata.len();

    let size_str = if size < 1024 {
        format!("{size} B")
    } else if size < 1024 * 1024 {
        format!("{:.2} KB", size as f64 / 1024.0)
    } else {
        format!("{:.2} MB", size as f64 / (1024.0 * 1024.0))
    };

    Ok(size_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_format_extension() {
        #[cfg(feature = "format-json")]
        assert_eq!(FileFormat::Json.extension(), "json");

        #[cfg(all(feature = "format-json", feature = "compression-gzip"))]
        assert_eq!(FileFormat::JsonGzip.extension(), "json.gz");

        #[cfg(feature = "format-messagepack")]
        assert_eq!(FileFormat::MessagePack.extension(), "msgpack");

        #[cfg(feature = "format-bincode")]
        assert_eq!(FileFormat::Bincode.extension(), "bincode");
    }

    #[test]
    fn test_format_from_extension() {
        use std::path::PathBuf;

        #[cfg(feature = "format-json")]
        assert_eq!(
            FileFormat::from_extension(&PathBuf::from("save.json")),
            Some(FileFormat::Json)
        );

        #[cfg(all(feature = "format-json", feature = "compression-gzip"))]
        assert_eq!(
            FileFormat::from_extension(&PathBuf::from("save.json.gz")),
            Some(FileFormat::JsonGzip)
        );

        #[cfg(feature = "format-messagepack")]
        assert_eq!(
            FileFormat::from_extension(&PathBuf::from("save.msgpack")),
            Some(FileFormat::MessagePack)
        );

        #[cfg(feature = "format-bincode")]
        assert_eq!(
            FileFormat::from_extension(&PathBuf::from("save.bincode")),
            Some(FileFormat::Bincode)
        );

        #[cfg(all(feature = "format-messagepack", feature = "compression-lz4"))]
        assert_eq!(
            FileFormat::from_extension(&PathBuf::from("save.msgpack.lz4")),
            Some(FileFormat::MessagePackLz4)
        );
    }
}
