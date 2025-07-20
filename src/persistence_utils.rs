//! 持久化便利函数
//! Persistence utility functions

use crate::persistence::{FogOfWarSaveData, PersistenceError};
use serde::{Deserialize, Serialize};
#[cfg(any(
    feature = "compression-gzip",
    feature = "compression-lz4",
    feature = "compression-zstd"
))]
use std::io::{Read, Write};
use std::path::Path;

/// 支持的文件格式
/// Supported file formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    /// JSON格式（可读，较大）
    /// JSON format (human-readable, larger)
    Json,
    /// JSON格式，使用gzip压缩
    /// JSON format with gzip compression
    #[cfg(feature = "compression-gzip")]
    JsonGzip,
    /// JSON格式，使用LZ4压缩（快速）
    /// JSON format with LZ4 compression (fast)
    #[cfg(feature = "compression-lz4")]
    JsonLz4,
    /// JSON格式，使用Zstandard压缩（高压缩率）
    /// JSON format with Zstandard compression (high compression ratio)
    #[cfg(feature = "compression-zstd")]
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
    /// 获取文件扩展名
    /// Get file extension
    pub fn extension(&self) -> &'static str {
        match self {
            FileFormat::Json => "json",
            #[cfg(feature = "compression-gzip")]
            FileFormat::JsonGzip => "json.gz",
            #[cfg(feature = "compression-lz4")]
            FileFormat::JsonLz4 => "json.lz4",
            #[cfg(feature = "compression-zstd")]
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

    /// 从文件扩展名推断格式
    /// Infer format from file extension
    pub fn from_extension(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;

        // 检查双扩展名（如 .json.gz, .msgpack.lz4等）
        // Check for double extensions (like .json.gz, .msgpack.lz4, etc.)
        if let Some(stem) = path.file_stem() {
            if let Some(stem_str) = stem.to_str() {
                if stem_str.ends_with(".json") {
                    match ext {
                        #[cfg(feature = "compression-gzip")]
                        "gz" => return Some(FileFormat::JsonGzip),
                        #[cfg(feature = "compression-lz4")]
                        "lz4" => return Some(FileFormat::JsonLz4),
                        #[cfg(feature = "compression-zstd")]
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
        }

        // 单扩展名
        // Single extension
        match ext {
            "json" => Some(FileFormat::Json),
            #[cfg(feature = "format-messagepack")]
            "msgpack" => Some(FileFormat::MessagePack),
            #[cfg(feature = "format-bincode")]
            "bincode" => Some(FileFormat::Bincode),
            _ => None,
        }
    }
}

/// 便利函数：保存数据到文件
/// Utility function: Save data to file
pub fn save_to_file(
    data: &str,
    path: impl AsRef<Path>,
    format: FileFormat,
) -> Result<(), PersistenceError> {
    let path = path.as_ref();

    match format {
        FileFormat::Json => {
            std::fs::write(path, data)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
        }

        #[cfg(feature = "compression-gzip")]
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
        }

        #[cfg(feature = "compression-lz4")]
        FileFormat::JsonLz4 => {
            let compressed = lz4::block::compress(data.as_bytes(), None, true)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            std::fs::write(path, compressed)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
        }

        #[cfg(feature = "compression-zstd")]
        FileFormat::JsonZstd => {
            let compressed = zstd::encode_all(data.as_bytes(), 3) // 压缩级别3（平衡）
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            std::fs::write(path, compressed)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
        }

        // 对于二进制格式，回退到通用处理
        // For binary formats, fall back to generic handling
        #[cfg(any(feature = "format-messagepack", feature = "format-bincode"))]
        _ => {
            // 这些格式应该通过save_data_to_file处理
            // These formats should be handled by save_data_to_file
            return Err(PersistenceError::SerializationFailed(format!(
                "Format {format:?} not supported by save_to_file, use save_data_to_file instead"
            )));
        }
    }

    Ok(())
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
            let bincode_data = bincode::serialize(data)
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
            let bincode_data = bincode::serialize(data)
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
            let bincode_data = bincode::serialize(data)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            let compressed = lz4::block::compress(&bincode_data, None, true)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            std::fs::write(path, compressed)
                .map_err(|e| PersistenceError::SerializationFailed(e.to_string()))?;
            Ok(())
        }

        #[cfg(all(feature = "format-bincode", feature = "compression-zstd"))]
        FileFormat::BincodeZstd => {
            let bincode_data = bincode::serialize(data)
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
    let format =
        format.unwrap_or_else(|| FileFormat::from_extension(path).unwrap_or(FileFormat::Json));

    match format {
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
            bincode::deserialize(&bincode_data)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))
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
            bincode::deserialize(&bincode_data)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))
        }

        #[cfg(all(feature = "format-bincode", feature = "compression-lz4"))]
        FileFormat::BincodeLz4 => {
            let compressed = std::fs::read(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let bincode_data = lz4::block::decompress(&compressed, None)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            bincode::deserialize(&bincode_data)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))
        }

        #[cfg(all(feature = "format-bincode", feature = "compression-zstd"))]
        FileFormat::BincodeZstd => {
            let compressed = std::fs::read(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let bincode_data = zstd::decode_all(&compressed[..])
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            bincode::deserialize(&bincode_data)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))
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
    let format =
        format.unwrap_or_else(|| FileFormat::from_extension(path).unwrap_or(FileFormat::Json));

    let data = match format {
        FileFormat::Json => std::fs::read_to_string(path)
            .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?,

        #[cfg(feature = "compression-gzip")]
        FileFormat::JsonGzip => {
            use flate2::read::GzDecoder;

            let file = std::fs::File::open(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let mut decoder = GzDecoder::new(file);
            let mut data = String::new();
            decoder
                .read_to_string(&mut data)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            data
        }

        #[cfg(feature = "compression-lz4")]
        FileFormat::JsonLz4 => {
            let compressed = std::fs::read(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let decompressed = lz4::block::decompress(&compressed, None)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            String::from_utf8(decompressed)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?
        }

        #[cfg(feature = "compression-zstd")]
        FileFormat::JsonZstd => {
            let compressed = std::fs::read(path)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            let decompressed = zstd::decode_all(&compressed[..])
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?;
            String::from_utf8(decompressed)
                .map_err(|e| PersistenceError::DeserializationFailed(e.to_string()))?
        }

        // 对于二进制格式，回退到通用处理
        // For binary formats, fall back to generic handling
        #[cfg(any(feature = "format-messagepack", feature = "format-bincode"))]
        _ => {
            // 这些格式应该通过load_data_from_file处理
            // These formats should be handled by load_data_from_file
            return Err(PersistenceError::DeserializationFailed(format!(
                "Format {format:?} not supported by load_from_file, use load_data_from_file instead"
            )));
        }
    };

    Ok(data)
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
        assert_eq!(FileFormat::Json.extension(), "json");

        #[cfg(feature = "compression-gzip")]
        assert_eq!(FileFormat::JsonGzip.extension(), "json.gz");

        #[cfg(feature = "format-messagepack")]
        assert_eq!(FileFormat::MessagePack.extension(), "msgpack");

        #[cfg(feature = "format-bincode")]
        assert_eq!(FileFormat::Bincode.extension(), "bincode");
    }

    #[test]
    fn test_format_from_extension() {
        use std::path::PathBuf;

        assert_eq!(
            FileFormat::from_extension(&PathBuf::from("save.json")),
            Some(FileFormat::Json)
        );

        #[cfg(feature = "compression-gzip")]
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
